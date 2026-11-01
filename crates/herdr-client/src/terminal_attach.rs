//! A direct terminal attach: Herdr's own `herdr terminal attach` handshake,
//! held only to size one terminal. While it is held, Herdr locks the
//! terminal's PTY to this client's columns and rows; dropping it detaches,
//! and Herdr sizes the terminal back to its layout.
//!
//! The attach never sends input. Herdr streams the attached client ANSI
//! frames, which a worker thread reads and discards so the server never
//! waits on this connection. The worker also owns every write after the
//! handshake, so one stream serves both directions on every platform.

mod error;

pub use error::Error;

use crate::{ConnectTarget, protocol::*, transport::Stream};
use std::{
    collections::VecDeque,
    io::{self, Read},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Herdr's same-install protocol version for direct terminal clients, as of
/// the vendored wire types. A Herdr that speaks another one says which in
/// its refusal, and the hello is tried once more with it.
pub const TERMINAL_PROTOCOL_VERSION: u32 = 22;

/// How long Herdr may take to answer the hello.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long Herdr may take to refuse the attach. Herdr sends nothing when
/// it accepts but the first frame, so silence past this counts as accepted.
const CONFIRM_TIMEOUT: Duration = Duration::from_millis(500);
/// How long the worker waits on a read before checking for a resize or a
/// stop; it bounds how long dropping the attach blocks.
const POLL: Duration = Duration::from_millis(20);
/// How long one write may block before the attach gives up.
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);
/// Scrolls waiting to be sent; consecutive ones the same way merge, so
/// only alternating directions fill it.
const MAX_SCROLLS: usize = 16;
/// The most lines one merged scroll moves.
const MAX_SCROLL_LINES: u16 = 1000;

/// A held direct attach. Drop detaches it.
#[derive(Debug)]
pub struct TerminalAttach {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

/// What the caller tells the worker, and whether the worker still runs.
#[derive(Debug)]
struct Shared {
    pending: Mutex<Pending>,
    stop: AtomicBool,
    ended: AtomicBool,
}

/// What waits for the worker to send.
#[derive(Debug)]
struct Pending {
    /// The size Herdr was last asked for; scrolls point at its middle.
    size: (u16, u16),
    /// The latest size asked for and not yet sent; a newer one replaces it.
    resize: Option<(u16, u16)>,
    /// Scrolls not yet sent, in order.
    scrolls: VecDeque<(AttachScrollDirection, u16)>,
}

impl TerminalAttach {
    /// Attaches to `terminal_id` on the local daemon `target` names, at
    /// `cols` by `rows`, without taking it from another direct client.
    /// Blocking: run it off any UI or async thread.
    pub fn open(
        target: &ConnectTarget,
        terminal_id: &str,
        cols: u16,
        rows: u16,
    ) -> crate::Result<Self> {
        let path = target.socket_path()?;
        Ok(Self::open_with(
            || Stream::connect(&path),
            terminal_id,
            cols,
            rows,
        )?)
    }

    /// [`open`](Self::open) over the streams `connect` makes: one per hello.
    pub(crate) fn open_with(
        mut connect: impl FnMut() -> io::Result<Stream>,
        terminal_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        if cols == 0 || rows == 0 {
            return Err(Error::Size);
        }
        let mut version = TERMINAL_PROTOCOL_VERSION;
        let mut stream = loop {
            let mut stream = connect()?;
            match hello(&mut stream, version, cols, rows) {
                Ok(()) => break stream,
                Err(Error::Rejected {
                    version: server, ..
                }) if server != version => {
                    tracing::debug!(
                        client = version,
                        server,
                        "retrying terminal hello at Herdr's version"
                    );
                    version = server;
                }
                Err(error) => return Err(error),
            }
        };
        // Set before asking: Herdr may refuse and close at once, and macOS
        // refuses a socket option on a socket its peer closed (`EINVAL`),
        // which would hide the refusal.
        stream.set_read_timeout(Some(CONFIRM_TIMEOUT))?;
        write_message(
            &mut stream,
            &ClientMessage::AttachTerminal {
                terminal_id: terminal_id.to_owned(),
                takeover: false,
            },
            MAX_FRAME_SIZE,
        )?;
        confirm(&mut stream)?;
        stream.set_read_timeout(Some(POLL))?;
        let shared = Arc::new(Shared {
            pending: Mutex::new(Pending {
                size: (cols, rows),
                resize: None,
                scrolls: VecDeque::new(),
            }),
            stop: AtomicBool::new(false),
            ended: AtomicBool::new(false),
        });
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("herdr-terminal-attach".into())
            .spawn(move || {
                drain(stream, &worker_shared);
                worker_shared.ended.store(true, Ordering::Release);
            })?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    /// Asks Herdr to size the terminal to `cols` by `rows`. Only the latest
    /// size waiting to be sent is kept.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        if cols == 0 || rows == 0 {
            return Err(Error::Size);
        }
        if !self.is_attached() {
            return Err(Error::Ended);
        }
        let mut pending = self.shared.pending();
        pending.size = (cols, rows);
        pending.resize = Some((cols, rows));
        Ok(())
    }

    /// Scrolls the terminal `lines` toward `direction` as a mouse wheel at
    /// its middle: Herdr hands it to an app that reports the mouse, else
    /// scrolls the terminal's own history. Consecutive scrolls the same
    /// way merge while they wait.
    pub fn scroll(&self, direction: AttachScrollDirection, lines: u16) -> Result<()> {
        if lines == 0 {
            return Err(Error::Scroll);
        }
        if !self.is_attached() {
            return Err(Error::Ended);
        }
        let scrolls = &mut self.shared.pending().scrolls;
        let full = scrolls.len() >= MAX_SCROLLS;
        match scrolls.back_mut() {
            Some((last, waiting)) if *last == direction => {
                *waiting = waiting.saturating_add(lines).min(MAX_SCROLL_LINES);
            }
            _ if full => return Err(Error::Busy),
            _ => scrolls.push_back((direction, lines.min(MAX_SCROLL_LINES))),
        }
        Ok(())
    }

    /// Whether Herdr still holds the attach: false once it closed the
    /// connection, as when the terminal exited or another client took it.
    pub fn is_attached(&self) -> bool {
        !self.shared.ended.load(Ordering::Acquire)
    }
}

impl Shared {
    fn pending(&self) -> std::sync::MutexGuard<'_, Pending> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What to send now, in order: a resize, then the scrolls.
    fn take(&self) -> Vec<ClientMessage> {
        let mut pending = self.pending();
        let (cols, rows) = pending.size;
        let resize = pending
            .resize
            .take()
            .map(|(cols, rows)| ClientMessage::Resize {
                cols,
                rows,
                cell_width_px: 0,
                cell_height_px: 0,
                pixel_mouse: false,
            });
        let scrolls =
            pending
                .scrolls
                .drain(..)
                .map(|(direction, lines)| ClientMessage::AttachScroll {
                    source: AttachScrollSource::Wheel,
                    direction,
                    lines,
                    column: Some(cols / 2),
                    row: Some(rows / 2),
                    modifiers: 0,
                });
        resize.into_iter().chain(scrolls).collect()
    }
}

impl Drop for TerminalAttach {
    /// Sends `Detach` and waits for the worker, at most one poll and one
    /// write timeout.
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::warn!("terminal attach worker panicked");
        }
    }
}

/// Sends the hello and reads Herdr's welcome.
fn hello(stream: &mut Stream, version: u32, cols: u16, rows: u16) -> Result<()> {
    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    write_message(
        stream,
        &ClientMessage::TerminalHello {
            version,
            cols,
            rows,
            cell_width_px: 0,
            cell_height_px: 0,
            pixel_mouse: false,
        },
        MAX_FRAME_SIZE,
    )?;
    match read_message(stream, MAX_FRAME_SIZE)? {
        ServerMessage::Welcome {
            error: Some(reason),
            version,
            ..
        } => Err(Error::Rejected {
            version,
            reason: reason.chars().take(512).collect(),
        }),
        ServerMessage::Welcome { error: None, .. } => Ok(()),
        _ => Err(Error::Unexpected),
    }
}

/// Waits up to the stream's read timeout, [`CONFIRM_TIMEOUT`], for Herdr
/// to refuse the attach. Any other frame, or none in time, means it took
/// it.
fn confirm(stream: &mut Stream) -> Result<()> {
    match read_message::<_, ServerMessage>(stream, MAX_GRAPHICS_FRAME_SIZE) {
        Ok(ServerMessage::ServerShutdown { reason }) => Err(Error::Refused(
            reason.map(|reason| reason.chars().take(512).collect()),
        )),
        // A frame these wire types do not decode was still read whole, so
        // the stream stays aligned for the worker.
        Ok(_) | Err(herdr_protocol::Error::Decode(_) | herdr_protocol::Error::TrailingBytes) => {
            Ok(())
        }
        Err(herdr_protocol::Error::Io(error)) if timed_out(&error) => Ok(()),
        Err(herdr_protocol::Error::Io(error)) if error.kind() == io::ErrorKind::UnexpectedEof => {
            Err(Error::Refused(None))
        }
        Err(error) => Err(error.into()),
    }
}

fn timed_out(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

/// Discards what Herdr sends and writes what the caller asked for, until
/// the caller stops (then sends `Detach`) or Herdr closes the connection.
fn drain(mut stream: Stream, shared: &Shared) {
    let mut buf = vec![0; 16 * 1024];
    loop {
        if shared.stop.load(Ordering::Acquire) {
            if let Err(error) = write_message(&mut stream, &ClientMessage::Detach, MAX_FRAME_SIZE) {
                tracing::debug!(%error, "terminal detach not sent");
            }
            return;
        }
        for message in shared.take() {
            if let Err(error) = write_message(&mut stream, &message, MAX_FRAME_SIZE) {
                tracing::debug!(%error, "terminal attach write failed");
                return;
            }
        }
        match stream.read(&mut buf) {
            Ok(0) => return,
            Ok(_) => {}
            Err(error) if timed_out(&error) => {}
            Err(error) => {
                tracing::debug!(%error, "terminal attach read failed");
                return;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
