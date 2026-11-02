//! A local endpoint only the user running this process can reach, for
//! programs of one user that talk to each other (the `herdr-acp` chat program
//! and the app attached to it).
//!
//! On Unix it is a socket file, mode 0600, in a directory of mode 0700. On
//! Windows it is a named pipe whose DACL admits only the user, created as the
//! first instance so a pipe someone else made is never joined, rejecting
//! remote clients; a client checks the pipe it reached is owned by the same
//! user before sending anything.
//!
//! A connection splits into a reader and a writer for two threads, and a
//! closer that makes the reader see the end of the stream from any thread.

use std::path::Path;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as platform;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[cfg(windows)]
pub(crate) use windows::IDENTIFICATION;
#[cfg(windows)]
pub use windows::{owner_sid, user_sid};

pub use platform::{Closer, Connection, Listener, Reader, Writer};

/// Removes what a listener left at `address`; a pipe needs nothing removed.
pub fn release(address: &Path) {
    platform::release(address);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
