//! FIFO reservations with an optional image lease held through transport.

mod error;
pub use error::Error;

use crate::{Result, frame::ImageWriter, limits::COMMAND_TIMEOUT, protocol::*, queue::Waker};
use crossbeam_channel::{Receiver, Sender};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

/// A FIFO slot reserved before starting asynchronous clipboard/file work.
/// Drop to skip the slot. Call `complete` on a background executor, never the UI.
#[must_use]
pub struct ClipboardImageUpload {
    pub(crate) target: ClientClipboardImageTarget,
    pub(crate) sender: Sender<Vec<u8>>,
    /// Fields drop in declaration order: this wakes the worker only once
    /// `sender` is gone, so it sees the published bytes or the closed slot.
    pub(crate) _wake: WakeOnDrop,
    pub(crate) lease: Arc<ImageLease>,
    pub(crate) stop: Arc<AtomicBool>,
}

/// Wakes the connection worker when dropped.
pub(crate) struct WakeOnDrop(pub(crate) Waker);

impl Drop for WakeOnDrop {
    fn drop(&mut self) {
        self.0.wake();
    }
}

/// Cancels only this upload. Keeping this handle does not retain the image lease.
#[derive(Clone)]
pub struct ClipboardImageCancellation {
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) finished: Arc<AtomicBool>,
    pub(crate) waker: Waker,
}

impl ClipboardImageCancellation {
    /// Nonblocking. If a frame has begun writing, cancellation closes the
    /// connection rather than leaving a truncated frame on a reusable stream.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.waker.wake();
    }

    /// True only once the worker's slot is gone (sent, rejected, or cancelled).
    /// Publication and requesting cancellation do not finish the slot. This is
    /// not daemon acknowledgement; retain this handle until it returns true.
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
}

impl ClipboardImageUpload {
    pub fn cancellation_handle(&self) -> ClipboardImageCancellation {
        ClipboardImageCancellation {
            cancelled: self.lease.cancelled.clone(),
            finished: self.lease.finished.clone(),
            waker: self.lease.waker.clone(),
        }
    }

    /// Useful between bounded background reads. Also true after worker rejection.
    pub fn is_cancelled(&self) -> bool {
        self.stop.load(Ordering::Acquire)
            || self.lease.cancelled.load(Ordering::Acquire)
            || self.lease.reserved_at.elapsed() >= COMMAND_TIMEOUT
    }

    /// Validate and encode at most 16 MiB, then publish without waiting for the
    /// worker. Success means queued, not daemon acknowledgement. The GUI must
    /// validate its captured connection epoch, boot, and target before calling.
    pub fn complete(self, extension: &str, data: Vec<u8>) -> Result<()> {
        self.publish(|upload| {
            upload.lease.claim_image()?;
            Ok(encode_clipboard_image(
                upload.target.clone(),
                extension,
                data,
            )?)
        })
    }

    /// Publish one original semantic event in this FIFO slot when no image is
    /// available. Encode in the background after the same GUI validation as
    /// `complete`. No terminal key or paste bytes are synthesized.
    pub fn complete_input(self, event: ClientPaneInputEvent) -> Result<()> {
        self.publish(|upload| {
            let events = vec![event];
            let message = match &upload.target {
                ClientClipboardImageTarget::Pane(pane_id) => ClientMessage::ClientShellPaneInput {
                    pane_id: pane_id.clone(),
                    events,
                },
                ClientClipboardImageTarget::Popup(terminal_id) => {
                    ClientMessage::ClientShellPopupInput {
                        terminal_id: terminal_id.clone(),
                        events,
                    }
                }
                ClientClipboardImageTarget::DirectTerminal => {
                    return Err(Error::InputTarget.into());
                }
            };
            Ok(encode_message(&message, MAX_FRAME_SIZE)?)
        })
    }

    fn publish(self, encode: impl FnOnce(&Self) -> Result<Vec<u8>>) -> Result<()> {
        self.check_ready()?;
        let bytes = encode(&self)?;
        self.check_ready()?;
        self.sender
            .try_send(bytes)
            .map_err(|_| crate::Error::Disconnected)
    }

    fn check_ready(&self) -> Result<()> {
        if self.stop.load(Ordering::Acquire) {
            return Err(crate::Error::Disconnected);
        }
        if self.is_cancelled() {
            return Err(Error::Cancelled.into());
        }
        Ok(())
    }
}

pub(crate) struct ImageLease {
    pub(crate) busy: Arc<AtomicBool>,
    pub(crate) claimed: AtomicBool,
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) finished: Arc<AtomicBool>,
    pub(crate) reserved_at: Instant,
    /// Publication and cancellation reach the worker outside its queue.
    pub(crate) waker: Waker,
}

impl ImageLease {
    // Only the reservation's producer claims; the worker only holds/drops it.
    pub(crate) fn claim_image(&self) -> Result<()> {
        if !self.claimed.load(Ordering::Acquire) {
            self.busy
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| Error::Busy)?;
            self.claimed.store(true, Ordering::Release);
        }
        Ok(())
    }
}

impl Drop for ImageLease {
    fn drop(&mut self) {
        if self.claimed.load(Ordering::Acquire) {
            self.busy.store(false, Ordering::Release);
        }
    }
}

pub(crate) struct ImageSlot {
    pub(crate) receiver: Receiver<Vec<u8>>,
    pub(crate) lease: Arc<ImageLease>,
    pub(crate) writer: ImageWriter,
}

impl ImageSlot {
    /// When the worker must act on this slot unwoken: preparation expires, or
    /// a started write times out. `published` is whether its bytes arrived.
    pub(crate) fn deadline(&self, published: bool) -> Option<Instant> {
        if published {
            self.writer.deadline()
        } else {
            Some(self.lease.reserved_at + COMMAND_TIMEOUT)
        }
    }
}

impl Drop for ImageSlot {
    fn drop(&mut self) {
        self.lease.cancelled.store(true, Ordering::Release);
        self.lease.finished.store(true, Ordering::Release);
    }
}
