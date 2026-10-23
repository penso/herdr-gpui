//! The control channel on Windows: a named pipe only the user running the
//! app can open. Pipe names are shared by every user on the machine and
//! anyone may create one first, so:
//!
//! - the name carries the user's SID, which keeps users apart, but does not
//!   by itself stop another user from creating it first;
//! - the server creates the pipe owned by the user, with a DACL granting
//!   only that user, as the first instance (so it never joins a pipe someone
//!   else made), and with remote clients rejected;
//! - the client opens it at identification level, so a server cannot act as
//!   the caller, and checks the pipe's owner is the caller before writing.
//!
//! A pipe has no receive timeout, so reads peek first, as the daemon client
//! does, and a pipe has no send timeout at all; the server's out buffer
//! holds a whole answer so its write does not wait on the caller reading.
/// The user and pipe owner checks, shared with the chat endpoint.
pub(super) use herdr_client::private_endpoint as security;

use super::super::protocol::MAX_MESSAGE;
use interprocess::os::windows::{
    ToWtf16 as _,
    named_pipe::{DuplexPipeStream, PipeListener, PipeListenerOptions, pipe_mode},
    security_descriptor::SecurityDescriptor,
};
use std::{
    fs::OpenOptions,
    io::{self, Read, Write},
    os::windows::{
        fs::OpenOptionsExt as _,
        io::{AsHandle as _, OwnedHandle},
    },
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const PREFIX: &str = r"\\.\pipe\";
/// How long an idle read first sleeps before peeking again. The sleep
/// doubles up to [`MAX_RETRY`]: an answer usually comes within
/// milliseconds, but a caller may wait minutes for the user.
const RETRY: Duration = Duration::from_millis(1);
const MAX_RETRY: Duration = Duration::from_millis(50);
/// `ERROR_PIPE_BUSY`: every instance is taken; the listener makes another.
const PIPE_BUSY: i32 = 231;
/// `FILE_FLAG_OVERLAPPED`, which `interprocess` opens its pipes with.
const OVERLAPPED: u32 = 0x4000_0000;
/// `SECURITY_IDENTIFICATION`: the server may learn who called, not act as them.
const IDENTIFICATION: u32 = 0x0001_0000;

type Pipe = DuplexPipeStream<pipe_mode::Bytes>;

/// The control pipe of the user running this process. A state directory
/// chosen with `XDG_STATE_HOME`, as tests and side-by-side installs do, gets
/// a pipe of its own; otherwise every shell of the user reaches the same
/// one, whatever `HOME` it was started with.
pub(super) fn address(state: &Path) -> crate::Result<PathBuf> {
    let sid = security::user_sid().map_err(crate::control::Error::User)?;
    let isolated = std::env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(|_| state);
    Ok(name(&sid, isolated))
}

pub(super) fn name(sid: &str, isolated: Option<&Path>) -> PathBuf {
    let mut name = format!("{PREFIX}herdr-gpui-control-{sid}");
    if let Some(state) = isolated {
        // One directory, however a shell spells it: Git Bash passes
        // `C:/state`, PowerShell `C:\state`, and Windows ignores case.
        let state = state
            .to_string_lossy()
            .replace('/', r"\")
            .trim_end_matches('\\')
            .to_lowercase();
        // FNV-1a: stable across builds, unlike the standard hasher.
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in state.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        name.push_str(&format!("-{hash:016x}"));
    }
    PathBuf::from(name)
}

pub(super) struct Listener(PipeListener<pipe_mode::Bytes, pipe_mode::Bytes>);

impl Listener {
    /// Creates the pipe. Another live server of this user keeps it, and a
    /// pipe another user made first is refused rather than joined.
    pub(super) fn bind(path: &Path) -> crate::Result<Self> {
        let sid = security::user_sid().map_err(crate::control::Error::User)?;
        // Owned by the user, protected from inherited entries, and open to
        // that user alone: no one else may connect or add an instance.
        let sddl = format!("O:{sid}D:P(A;;GA;;;{sid})");
        let descriptor =
            SecurityDescriptor::deserialize(&sddl.to_wtf_16().map_err(io::Error::other)?)?;
        let created = PipeListenerOptions::new()
            .path(path)
            .accept_remote(false)
            .security_descriptor(Some(descriptor))
            // A whole answer fits, so writing it never waits on the caller.
            .output_buffer_size_hint(u32::try_from(MAX_MESSAGE + 1).unwrap_or(u32::MAX))
            .create_duplex::<pipe_mode::Bytes>();
        match created {
            Ok(listener) => Ok(Self(listener)),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                // The name exists: ours if a server of this user answers.
                match connect(path, Duration::from_secs(1), Duration::from_secs(1)) {
                    Ok(_) => Err(crate::control::Error::SocketInUse {
                        path: path.to_owned(),
                    }
                    .into()),
                    Err(crate::Error::Control(crate::control::Error::Unavailable { .. })) => {
                        Err(crate::control::Error::PipeForeign {
                            path: path.to_owned(),
                        }
                        .into())
                    }
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    pub(super) fn accept(&self) -> io::Result<Connection> {
        Ok(Connection {
            pipe: self.0.accept()?,
            read_timeout: Duration::ZERO,
        })
    }
}

/// Nothing to remove: a pipe goes away with its last handle.
pub(super) fn release(_: &Path) {}

pub(super) struct Connection {
    pub(super) pipe: Pipe,
    read_timeout: Duration,
}

impl Connection {
    /// Bounds reads. Writes have no timeout on a pipe; see the module.
    pub(super) fn prepare(&mut self, timeout: Duration) -> io::Result<()> {
        self.read_timeout = timeout;
        Ok(())
    }

    /// Peeking needs no mode change, unlike a socket's nonblocking read.
    pub(super) fn watch(&self, _: bool) -> io::Result<()> {
        Ok(())
    }

    /// A request is one line only, so a closed pipe, another byte, or an
    /// error all end it.
    pub(super) fn peer_closed(&self) -> bool {
        !matches!(
            herdr_client::pipe_available(self.pipe.as_handle()),
            Ok(Some(0))
        )
    }
}

impl Read for &Connection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let deadline = Instant::now() + self.read_timeout;
        let mut retry = RETRY;
        loop {
            match herdr_client::pipe_available(self.pipe.as_handle())? {
                // A closed peer reads as end of stream, as a socket does.
                None => return Ok(0),
                Some(0) => {}
                // The pipe holds these bytes already, so this cannot block.
                Some(ready) => {
                    let len = buf.len().min(ready as usize);
                    return (&self.pipe).read(&mut buf[..len]);
                }
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            thread::sleep(retry.min(left));
            retry = (retry * 2).min(MAX_RETRY);
        }
    }
}

impl Write for &Connection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.pipe).write(buf)
    }

    /// Flushing a pipe waits until the peer has read everything, which a
    /// stalled peer never does. Written bytes are already in the pipe, and
    /// `interprocess` keeps a dropped end open until they are read.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Connects to the server at `path`, waiting up to `io_timeout` for a free
/// instance, and checks it belongs to this user before anything is sent.
pub(super) fn connect(
    path: &Path,
    io_timeout: Duration,
    read_timeout: Duration,
) -> crate::Result<Connection> {
    let unavailable = |source| {
        crate::Error::from(crate::control::Error::Unavailable {
            path: path.to_owned(),
            source,
        })
    };
    let until = Instant::now() + io_timeout;
    let file = loop {
        let opened = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(OVERLAPPED)
            .security_qos_flags(IDENTIFICATION)
            .open(path);
        match opened {
            Ok(file) => break file,
            Err(error) if error.raw_os_error() == Some(PIPE_BUSY) && Instant::now() < until => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(unavailable(error)),
        }
    };
    let pipe =
        Pipe::try_from(OwnedHandle::from(file)).map_err(|error| unavailable(error.into()))?;
    // Nothing is written yet. A wrapped handle otherwise counts as unflushed
    // and lingers after a drop, so the server would not see this end close.
    pipe.assume_flushed();
    let owner = security::owner_sid(pipe.as_handle()).map_err(unavailable)?;
    let user = security::user_sid().map_err(crate::control::Error::User)?;
    if owner != user {
        return Err(crate::control::Error::PipeForeign {
            path: path.to_owned(),
        }
        .into());
    }
    Ok(Connection { pipe, read_timeout })
}
