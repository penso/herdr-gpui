//! The Windows endpoint: a named pipe only the user can open.
//!
//! A pipe has no receive timeout, and a blocking read would hold its thread
//! until the peer writes, so reads peek first and sleep while nothing is
//! buffered, as the daemon client does. Reads and writes go through shared
//! references to one pipe from two threads.
//!
//! The client end stays a plain synchronous file. Wrapping it in an
//! `interprocess` stream reopens the handle by name, which joins the next
//! free instance the listener made and closes the one the server accepted,
//! and drops the identification-only security level it was opened with.
//! Reads never wait on a synchronous handle (they follow a peek), so a read
//! queued behind a write lasts only as long as the server takes to drain
//! the pipe, which its own reader thread does continuously.

mod security;

pub use security::{owner_sid, user_sid};

use interprocess::os::windows::{
    ToWtf16 as _,
    named_pipe::{DuplexPipeStream, PipeListener, PipeListenerOptions, pipe_mode},
    security_descriptor::SecurityDescriptor,
};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::windows::{
        fs::OpenOptionsExt as _,
        io::{AsHandle, BorrowedHandle},
    },
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

type Pipe = DuplexPipeStream<pipe_mode::Bytes>;

/// How long an idle read first sleeps before peeking again; it doubles up to
/// [`MAX_RETRY`].
const RETRY: Duration = Duration::from_millis(1);
const MAX_RETRY: Duration = Duration::from_millis(25);
/// `ERROR_PIPE_BUSY`: every instance is taken; the listener makes another.
const PIPE_BUSY: i32 = 231;
/// `SECURITY_IDENTIFICATION`: the server may learn who called, not act as them.
pub(crate) const IDENTIFICATION: u32 = 0x0001_0000;

pub struct Listener(PipeListener<pipe_mode::Bytes, pipe_mode::Bytes>);

impl Listener {
    /// Creates the pipe named `address`, such as `\\.\pipe\name`. A name that
    /// already exists, whoever made it, fails with `AddrInUse`.
    pub fn bind(address: &Path) -> io::Result<Self> {
        let sid = user_sid()?;
        // Owned by the user, protected from inherited entries, and open to
        // that user alone: no one else may connect or add an instance.
        let sddl = format!("O:{sid}D:P(A;;GA;;;{sid})");
        let descriptor =
            SecurityDescriptor::deserialize(&sddl.to_wtf_16().map_err(io::Error::other)?)?;
        let created = PipeListenerOptions::new()
            .path(address)
            .accept_remote(false)
            .security_descriptor(Some(descriptor))
            .create_duplex::<pipe_mode::Bytes>();
        match created {
            Ok(listener) => Ok(Self(listener)),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                Err(io::ErrorKind::AddrInUse.into())
            }
            Err(error) => Err(error),
        }
    }

    pub fn accept(&self) -> io::Result<Connection> {
        Ok(Connection::new(End::Server(self.0.accept()?)))
    }
}

pub(super) fn release(_: &Path) {}

/// One end of the pipe: the server's `interprocess` stream, or the client's
/// synchronous file (see the module).
enum End {
    Client(File),
    Server(Pipe),
}

impl AsHandle for End {
    fn as_handle(&self) -> BorrowedHandle<'_> {
        match self {
            Self::Client(file) => file.as_handle(),
            Self::Server(pipe) => pipe.as_handle(),
        }
    }
}

impl Read for &End {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            End::Client(file) => (&*file).read(buf),
            End::Server(pipe) => (&*pipe).read(buf),
        }
    }
}

impl Write for &End {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            End::Client(file) => (&*file).write(buf),
            End::Server(pipe) => (&*pipe).write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct Connection {
    closed: Arc<AtomicBool>,
    pipe: Arc<End>,
}

impl Connection {
    fn new(end: End) -> Self {
        Self {
            closed: Arc::new(AtomicBool::new(false)),
            pipe: Arc::new(end),
        }
    }

    /// Opens the pipe at `address`, waiting up to `timeout` for a free
    /// instance, and refuses one that another user owns.
    pub fn connect(address: &Path, timeout: Duration) -> io::Result<Self> {
        let until = Instant::now() + timeout;
        let file = loop {
            let opened = OpenOptions::new()
                .read(true)
                .write(true)
                .security_qos_flags(IDENTIFICATION)
                .open(address);
            match opened {
                Ok(file) => break file,
                Err(error) if error.raw_os_error() == Some(PIPE_BUSY) && Instant::now() < until => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        };
        if owner_sid(file.as_handle())? != user_sid()? {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(Self::new(End::Client(file)))
    }

    pub fn split(self) -> io::Result<(Reader, Writer, Closer)> {
        Ok((
            Reader {
                closed: Arc::clone(&self.closed),
                pipe: Arc::clone(&self.pipe),
                timeout: None,
            },
            Writer {
                closed: Arc::clone(&self.closed),
                pipe: Arc::clone(&self.pipe),
            },
            Closer {
                closed: self.closed,
            },
        ))
    }
}

pub struct Reader {
    closed: Arc<AtomicBool>,
    pipe: Arc<End>,
    timeout: Option<Duration>,
}

impl Reader {
    /// Bounds each read; `None` waits as long as it takes.
    pub fn set_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.timeout = timeout;
        Ok(())
    }
}

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let deadline = self.timeout.map(|timeout| Instant::now() + timeout);
        let mut retry = RETRY;
        loop {
            if self.closed.load(Ordering::Acquire) {
                return Ok(0);
            }
            match crate::pipe_available(self.pipe.as_handle())? {
                // A closed peer reads as end of stream, as a socket does.
                None => return Ok(0),
                Some(0) => {}
                // The pipe holds these bytes already, so this cannot block.
                Some(ready) => {
                    let len = buf.len().min(ready as usize);
                    return (&*self.pipe).read(&mut buf[..len]);
                }
            }
            let mut pause = retry;
            if let Some(deadline) = deadline {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                pause = pause.min(left);
            }
            thread::sleep(pause);
            retry = (retry * 2).min(MAX_RETRY);
        }
    }
}

pub struct Writer {
    closed: Arc<AtomicBool>,
    pipe: Arc<End>,
}

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.closed.load(Ordering::Acquire) {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        (&*self.pipe).write(buf)
    }

    /// Flushing a pipe waits until the peer has read everything, which a
    /// stalled peer never does. Written bytes are already in the pipe.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct Closer {
    closed: Arc<AtomicBool>,
}

impl Closer {
    /// Ends the connection both ways: the reader sees the end of the stream
    /// and further writes fail. The pipe itself closes with its last handle.
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            closed: Arc::clone(&self.closed),
        })
    }
}
