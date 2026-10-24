//! Incremental frame reads that survive socket timeouts. Restarting a partial
//! read would silently corrupt framing, so the prefix and payload are retained.

use crate::{
    Error, Result, clipboard,
    limits::{COMMAND_TIMEOUT, POLL},
    protocol::*,
};
use std::{
    io::{self, Read, Write},
    time::{Duration, Instant},
};

/// Only one bounded write attempt per poll; never restart a partial frame.
#[derive(Default)]
pub(crate) struct ImageWriter {
    pub(crate) offset: usize,
    pub(crate) started: Option<Instant>,
}

impl ImageWriter {
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.started.map(|started| started + COMMAND_TIMEOUT)
    }

    pub(crate) fn check_timeout(&self) -> Result<()> {
        if self.deadline().is_some_and(|due| Instant::now() >= due) {
            return Err(clipboard::Error::WriteTimeout.into());
        }
        Ok(())
    }

    pub(crate) fn poll(&mut self, stream: &mut impl Write, bytes: &[u8]) -> Result<bool> {
        self.check_timeout()?;
        self.write_some(stream, bytes)
    }

    /// One bounded write attempt resuming at `offset`; the caller owns the deadline.
    pub(crate) fn write_some(&mut self, stream: &mut impl Write, bytes: &[u8]) -> Result<bool> {
        self.started.get_or_insert_with(Instant::now);
        let end = bytes.len().min(self.offset + 64 * 1024);
        match stream.write(&bytes[self.offset..end]) {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero).into()),
            Ok(n) => self.offset += n,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.into()),
        }
        Ok(self.offset == bytes.len())
    }
}

/// Keep partially read prefixes/payloads across socket timeouts. Restarting
/// read_exact after a timeout would silently corrupt framing.
pub(crate) struct FrameReader {
    pub(crate) bytes: Vec<u8>,
    pub(crate) target: usize,
    pub(crate) started: Option<Instant>,
    /// How long one frame may take to arrive once its first byte has.
    timeout: Duration,
}
impl FrameReader {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::with_timeout(crate::limits::TIMEOUT)
    }

    pub(crate) fn with_timeout(timeout: Duration) -> Self {
        Self {
            bytes: Vec::new(),
            target: 4,
            started: None,
            timeout,
        }
    }

    /// Identity only; the unfinished body remains unvalidated and unpublished.
    pub(crate) fn has_current_surface_prefix(
        &self,
        snapshot: &ClientShellSnapshot,
        surface: &PaneSurfaceFrame,
    ) -> bool {
        self.deadline().is_some_and(|due| Instant::now() < due)
            && self.target > 4
            && self.bytes.len() < self.target
            && self
                .bytes
                .get(4..)
                .is_some_and(|payload| has_current_surface_prefix(payload, snapshot, surface))
    }

    /// Drain a progressing frame before retrying a backpressured write. Stop at
    /// one complete frame, 1 MiB, or one poll interval, whichever comes first.
    /// An idle/interrupting read yields immediately, not 128 receive timeouts.
    pub(crate) fn poll_batch(
        &mut self,
        stream: &mut (impl Read + ?Sized),
    ) -> Result<Option<ServerMessage>> {
        let started = Instant::now();
        for _ in 0..128 {
            let previous = self.bytes.len();
            if let Some(message) = self.poll(stream)? {
                return Ok(Some(message));
            }
            if self.bytes.len() == previous || started.elapsed() >= POLL {
                break;
            }
        }
        Ok(None)
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.started.map(|started| started + self.timeout)
    }

    /// The worker calls this on every pass, so a peer that stops mid-frame
    /// fails the connection even though no further read happens.
    pub(crate) fn check_timeout(&self) -> Result<()> {
        if self.deadline().is_some_and(|due| Instant::now() >= due) {
            tracing::warn!(category = "partial_frame", "client timeout");
            return Err(Error::PartialFrameTimeout);
        }
        Ok(())
    }

    pub(crate) fn poll(
        &mut self,
        stream: &mut (impl Read + ?Sized),
    ) -> Result<Option<ServerMessage>> {
        let mut buf = [0; 8192];
        let len = buf.len().min(self.target - self.bytes.len());
        match stream.read(&mut buf[..len]) {
            Ok(0) => {
                return Err(Error::SocketClosed);
            }
            Ok(n) => {
                self.started.get_or_insert_with(Instant::now);
                self.bytes.extend_from_slice(&buf[..n]);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        }
        if self.bytes.len() != self.target {
            return Ok(None);
        }
        if self.target == 4 {
            let prefix = self
                .bytes
                .as_slice()
                .try_into()
                .map_err(|_| Error::FramePrefix)?;
            let len = u32::from_le_bytes(prefix) as usize;
            if len == 0 || len > MAX_GRAPHICS_FRAME_SIZE {
                return Err(Error::FrameLength);
            }
            self.target += len;
            return Ok(None);
        }
        let message = decode_payload(&self.bytes[4..])?;
        self.bytes.clear();
        self.target = 4;
        self.started = None;
        Ok(Some(message))
    }
}
