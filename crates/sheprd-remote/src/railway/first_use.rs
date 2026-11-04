//! The one connection that creates a Railway VM.
//!
//! Railway makes a VM for a new key only from an SSH session that has a real
//! terminal, so this runs `ssh` on a pseudo-terminal of a fixed 80x32 size and
//! lets the remote command finish. It is not a terminal emulator: nothing is
//! drawn, no input is sent, and what the VM prints is kept only as a bounded,
//! sanitized tail for a failure message.
//!
//! It runs only when the user starts setup for a key whose VM does not exist
//! (a headless probe is refused). Reconnects never call it, and the key is the
//! one setup already holds, so it can only create or reopen that key's VM.

use super::{Error, Result};
use crate::cloud::install::Finished;
use std::process::Command;

/// The terminal size `ssh` reports to Railway. Railway needs a non-zero size.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) const COLUMNS: u16 = 80;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) const ROWS: u16 = 32;

/// Run `session` (an `ssh` command that already ends in its remote command)
/// on a pseudo-terminal until it exits, is cancelled, or times out.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(super) fn create_vm(session: Command, cancelled: &dyn Fn() -> bool) -> Result<Finished> {
    use crate::cloud::install;
    use nix::pty::{Winsize, openpty};
    use std::{
        fs::File,
        process::Stdio,
        sync::{Arc, Mutex},
        time::Duration,
    };

    /// Railway answers in a few seconds; this bounds a stuck session.
    const TIMEOUT: Duration = Duration::from_secs(90);

    let size = Winsize {
        ws_row: ROWS,
        ws_col: COLUMNS,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let pty = openpty(&size, None).map_err(|errno| Error::Terminal(errno.into()))?;
    let mut session = session;
    let slave = |fd: &std::os::fd::OwnedFd| fd.try_clone().map(Stdio::from);
    let child = session
        .stdin(slave(&pty.slave).map_err(Error::Terminal)?)
        .stdout(slave(&pty.slave).map_err(Error::Terminal)?)
        .stderr(slave(&pty.slave).map_err(Error::Terminal)?)
        .spawn()
        .map_err(|error| Error::from(crate::cloud::Error::Process(error)))?;
    // The child holds the only other copies of the slave: once it is gone the
    // master reads end instead of blocking forever.
    drop(session);
    drop(pty.slave);
    let master = File::from(pty.master);
    let output = Arc::new(Mutex::new(Vec::new()));
    let reader = install::drain(master, output.clone());
    match install::supervise(child, [Some(reader)], &output, TIMEOUT, &cancelled) {
        Err(crate::cloud::Error::InstallTimeout) => Err(Error::CreateTimeout),
        finished => Ok(finished?),
    }
}

/// Creating the VM needs a pseudo-terminal this platform's build lacks.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn create_vm(_: Command, _: &dyn Fn() -> bool) -> Result<Finished> {
    Err(Error::TerminalUnsupported)
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
