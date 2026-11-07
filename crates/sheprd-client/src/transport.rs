//! The client's local stream type, matching the endpoint each platform's Herdr
//! daemon actually binds: a filesystem `AF_UNIX` socket on Unix, and a named
//! pipe on Windows, where upstream maps the same socket path string into the
//! NPFS namespace. Discovery, framing, and the session loop stay identical; only
//! the stream underneath differs. On Windows a stream can also be a bridge
//! child's standard streams. Both are pipes driven with overlapped I/O, which
//! gives them the receive and send timeouts a socket has.

#[cfg(unix)]
pub use std::os::unix::net::UnixStream as Stream;

#[cfg(all(test, unix))]
pub(crate) use std::os::unix::net::UnixListener as Listener;

#[cfg(windows)]
pub use windows::{Stream, available as pipe_available};

#[cfg(windows)]
mod overlapped;
#[cfg(windows)]
mod windows;

#[cfg(all(test, windows))]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
