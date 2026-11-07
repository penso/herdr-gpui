//! The Windows stream: the daemon's named pipe, or a bridge child's standard
//! streams, both driven with overlapped I/O so each read and write waits no
//! longer than the timeout its caller set.
//!
//! A write behaves like a send into a socket buffer one chunk deep. It copies
//! up to one chunk into an operation it starts and reports those bytes
//! written; the next write, or a flush, first waits for that operation, but
//! only until its own deadline. A deadline that passes leaves the operation in
//! flight with its bytes, so a timed-out call has written nothing and a caller
//! can retry it without losing or repeating a byte: framing survives every
//! timeout. Only dropping the stream cancels, which ends the connection.

use super::overlapped::{CHUNK, Channel, Progress};
use interprocess::local_socket::{
    GenericNamespaced, Stream as LocalStream, ToNsName, traits::Stream as _,
};
use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    ops::Range,
    os::windows::{
        ffi::OsStrExt,
        fs::OpenOptionsExt,
        io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    process::Stdio,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::INVALID_HANDLE_VALUE,
    Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX,
        PIPE_ACCESS_INBOUND, PIPE_ACCESS_OUTBOUND,
    },
    System::Pipes::{
        CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_WAIT, PeekNamedPipe,
    },
};

/// A connected Herdr client endpoint, with the timeouts the caller set, which
/// a pipe cannot hold itself.
#[derive(Debug)]
pub struct Stream {
    reader: Reader,
    writer: Writer,
}

struct Reader {
    channel: Channel,
    /// Bytes a completed read left in the channel's buffer, not yet returned.
    ready: Range<usize>,
    closed: bool,
    timeout: Option<Duration>,
}

struct Writer {
    channel: Channel,
    /// The length of the write in flight, which `write` already reported.
    in_flight: Option<usize>,
    timeout: Option<Duration>,
}

impl std::fmt::Debug for Reader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader")
            .field("ready", &self.ready.len())
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Writer")
            .field("in_flight", &self.in_flight)
            .finish_non_exhaustive()
    }
}

/// Upstream derives the pipe name from the socket path exactly this way, so
/// both ends agree without either of them owning a file at that path.
fn connect(path: &Path) -> io::Result<OwnedHandle> {
    let name = path.to_string_lossy().into_owned();
    let name = name.to_ns_name::<GenericNamespaced>().inspect_err(|error| {
        tracing::warn!(operation = "pipe_name", kind = ?error.kind(), raw_os_error = ?error.raw_os_error(), "Windows local transport failed");
    })?;
    let LocalStream::NamedPipe(pipe) = LocalStream::connect(name).inspect_err(|error| {
        // Keep native codes without recording the user's socket path. The
        // byte-mode connector opens the pipe overlapped, in its read mode.
        tracing::warn!(operation = "pipe_connect", kind = ?error.kind(), raw_os_error = ?error.raw_os_error(), "Windows local transport failed");
    })?;
    // Owning the handle also keeps `interprocess` from flushing a dropped
    // stream on a thread of its own, which would wait on the peer unbounded.
    Ok(pipe.into())
}

/// Whether an error means the pipe is gone rather than merely idle. Mirrors
/// the set the daemon's own client treats as a disconnect, including the raw
/// codes whose `ErrorKind` mapping is not specific enough.
fn closed(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotConnected
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::WriteZero
    ) || matches!(error.raw_os_error(), Some(6 | 109 | 232 | 233))
}

/// Bytes already buffered on a pipe, or `None` once the peer closed it, for
/// callers that hold a synchronous pipe of their own (the app's control pipe).
/// Reading an idle synchronous pipe would block with no deadline, and
/// `PIPE_NOWAIT` reports "no data yet" as a disconnect, so peeking first is the
/// only bounded way to read one; no safe wrapper exposes it.
#[allow(unsafe_code)]
pub fn available(pipe: BorrowedHandle<'_>) -> io::Result<Option<u32>> {
    let mut ready: u32 = 0;
    // SAFETY: the handle is borrowed from a pipe that outlives this call.
    // `lpBuffer`/`lpBytesRead`/`lpBytesLeftThisMessage` are documented as
    // optional and passed as null with a zero buffer size, so nothing is
    // copied out. `ready` is a live, exclusively borrowed `u32` that the call
    // writes only on success, and the return value is checked before it is
    // read.
    let peeked = unsafe {
        PeekNamedPipe(
            pipe.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut ready,
            std::ptr::null_mut(),
        )
    };
    if peeked != 0 {
        return Ok(Some(ready));
    }
    let error = io::Error::last_os_error();
    if closed(&error) {
        Ok(None)
    } else {
        tracing::warn!(operation = "pipe_peek", kind = ?error.kind(), raw_os_error = ?error.raw_os_error(), "Windows local transport failed");
        Err(error)
    }
}

fn deadline(timeout: Option<Duration>) -> Option<Instant> {
    timeout.map(|timeout| Instant::now() + timeout)
}

/// A socket refuses a zero timeout the same way.
fn validate(timeout: Option<Duration>) -> io::Result<()> {
    if timeout == Some(Duration::ZERO) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

/// Which way a bridge pipe carries bytes, seen from this process.
#[derive(Clone, Copy)]
enum Direction {
    Duplex,
    /// This process reads; the child's end writes.
    Inbound,
    /// This process writes; the child's end reads.
    Outbound,
}

/// Create a private pipe and open its other end, returning this process's end
/// (overlapped) and the other one (synchronous, as a child's standard stream
/// expects; `overlapped_peer` is for tests that drive both ends here).
///
/// This is what the standard library does for `Stdio::piped`, made explicit
/// because its ends are not documented to be overlapped: a unique name, one
/// instance only, created first and refusing remote clients, so the open
/// below is the only client that can ever reach it. A process that raced it
/// would make that open fail rather than share the pipe.
fn private_pipe(direction: Direction, overlapped_peer: bool) -> io::Result<(OwnedHandle, File)> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    let name = format!(
        r"\\.\pipe\herdr-client-{}-{}-{nanos}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let wide: Vec<u16> = OsStr::new(&name).encode_wide().chain([0]).collect();
    let access = match direction {
        Direction::Duplex => PIPE_ACCESS_DUPLEX,
        Direction::Inbound => PIPE_ACCESS_INBOUND,
        Direction::Outbound => PIPE_ACCESS_OUTBOUND,
    };
    #[allow(unsafe_code)]
    // SAFETY: `wide` is a NUL-terminated UTF-16 name that outlives the call,
    // and null security attributes select the default descriptor, as the
    // standard library's own pipes do. The result is checked, then owned.
    let handle = unsafe {
        let handle = CreateNamedPipeW(
            wide.as_ptr(),
            access | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            CHUNK as u32,
            CHUNK as u32,
            0,
            std::ptr::null(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        OwnedHandle::from_raw_handle(handle)
    };
    let mut options = OpenOptions::new();
    match direction {
        Direction::Duplex => options.read(true).write(true),
        Direction::Inbound => options.write(true),
        Direction::Outbound => options.read(true),
    };
    if overlapped_peer {
        options.custom_flags(FILE_FLAG_OVERLAPPED);
    }
    Ok((handle, options.open(&name)?))
}

impl Stream {
    pub fn connect(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::duplex(connect(path.as_ref())?)
    }

    /// Opens the pipe at the full `\\.\pipe\...` path, as a program of this
    /// user serves it, and refuses one another user owns. A pipe name is open
    /// to every local account, so one could create it first and wait for the
    /// client; and the server may only identify this client, never act as it.
    pub fn connect_owned(path: impl AsRef<Path>) -> io::Result<Self> {
        let pipe = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(FILE_FLAG_OVERLAPPED)
            .security_qos_flags(crate::private_endpoint::IDENTIFICATION)
            .open(path.as_ref())?;
        let owner = crate::private_endpoint::owner_sid(pipe.as_handle())?;
        if owner != crate::private_endpoint::user_sid()? {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Self::duplex(OwnedHandle::from(pipe))
    }

    /// A bridge child's standard streams: this end, and the child's stdin and
    /// stdout. Drop the command once the child is spawned; until then it holds
    /// the child's ends, and an exited child would never read as closed.
    pub(crate) fn child() -> io::Result<(Self, Stdio, Stdio)> {
        let (writer, stdin) = private_pipe(Direction::Outbound, false)?;
        let (reader, stdout) = private_pipe(Direction::Inbound, false)?;
        Ok((Self::new(reader, writer)?, stdin.into(), stdout.into()))
    }

    /// One overlapped duplex handle, read and written through two handles to
    /// it so that each direction has its own operation.
    fn duplex(handle: OwnedHandle) -> io::Result<Self> {
        Self::new(handle.try_clone()?, handle)
    }

    fn new(reader: OwnedHandle, writer: OwnedHandle) -> io::Result<Self> {
        Ok(Self {
            reader: Reader {
                channel: Channel::new(reader)?,
                ready: 0..0,
                closed: false,
                timeout: None,
            },
            writer: Writer {
                channel: Channel::new(writer)?,
                in_flight: None,
                timeout: None,
            },
        })
    }

    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        validate(timeout)?;
        self.reader.timeout = timeout;
        Ok(())
    }

    /// Bounds each write and flush, as a socket's send timeout does.
    pub fn set_write_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        validate(timeout)?;
        self.writer.timeout = timeout;
        Ok(())
    }

    /// A connected pair over a private pipe, for tests.
    pub fn pair() -> io::Result<(Self, Self)> {
        let (server, client) = private_pipe(Direction::Duplex, true)?;
        Ok((Self::duplex(server)?, Self::duplex(client.into())?))
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let reader = &mut self.reader;
        if buf.is_empty() {
            return Ok(0);
        }
        let deadline = deadline(reader.timeout);
        while reader.ready.is_empty() {
            // Report a closed peer as end of stream, as a socket does.
            if reader.closed {
                return Ok(0);
            }
            if !reader.channel.is_pending() {
                match reader.channel.start_read() {
                    Ok(()) => {}
                    Err(error) if closed(&error) => reader.closed = true,
                    Err(error) => return Err(error),
                }
                continue;
            }
            match reader.channel.wait(deadline) {
                // The read stays in flight with its buffer; a later call
                // collects whatever it receives meanwhile.
                Ok(Progress::Pending) => return Err(io::ErrorKind::TimedOut.into()),
                Ok(Progress::Done(len)) => reader.ready = 0..len,
                Err(error) if closed(&error) => reader.closed = true,
                Err(error) => {
                    tracing::warn!(operation = "pipe_read", kind = ?error.kind(), raw_os_error = ?error.raw_os_error(), "Windows local transport failed");
                    return Err(error);
                }
            }
        }
        let len = buf.len().min(reader.ready.len());
        let start = reader.ready.start;
        buf[..len].copy_from_slice(&reader.channel.buffer()[start..start + len]);
        reader.ready.start += len;
        Ok(len)
    }
}

impl Writer {
    /// Wait until the write in flight lands, or report the deadline.
    fn settle(&mut self, deadline: Option<Instant>) -> io::Result<()> {
        let Some(expected) = self.in_flight else {
            return Ok(());
        };
        let progress = self.channel.wait(deadline);
        if !matches!(progress, Ok(Progress::Pending)) {
            self.in_flight = None;
        }
        match progress? {
            Progress::Pending => Err(io::ErrorKind::TimedOut.into()),
            Progress::Done(written) if written == expected => Ok(()),
            // Bytes already reported written were dropped: the stream is no
            // longer framed, so it must not carry anything else.
            Progress::Done(_) => Err(io::ErrorKind::WriteZero.into()),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let writer = &mut self.writer;
        writer.settle(deadline(writer.timeout))?;
        let written = writer.channel.start_write(buf)?;
        writer.in_flight = Some(written);
        Ok(written)
    }

    /// Wait until every byte reported written is in the pipe, within the write
    /// timeout. A deadline here, too, leaves the write in flight.
    fn flush(&mut self) -> io::Result<()> {
        let writer = &mut self.writer;
        writer.settle(deadline(writer.timeout))
    }
}
