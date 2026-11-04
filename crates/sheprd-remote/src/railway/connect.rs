//! Reaching a saved Railway device: the device's account is the path of its SSH
//! key, which must still be the key the device was set up with, and Herdr's
//! stdio bridge runs through `ssh railway.new` with it. Runs on the connection
//! worker, and the UI retries it on its own, so this must be safe to repeat: it
//! never generates a key, never creates a VM or a terminal session, never
//! installs anything, and never falls back to another identity. Making the VM
//! ready is setup's job alone.

use super::{Error, Result, keys, ssh};
use sheprd_client::{Bridge, Transport};
use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

fn connect_saved(
    dir: &Path,
    account: &str,
    id: &str,
    session: &str,
    stop: &AtomicBool,
) -> Result<Bridge> {
    let command = saved_command(dir, account, id, stop)?;
    sheprd_client::connect_command(command, session, stop).map_err(Error::Bridge)
}

fn saved_command(dir: &Path, account: &str, id: &str, stop: &AtomicBool) -> Result<Command> {
    let cancelled = || stop.load(Ordering::Acquire);
    let key = PathBuf::from(account);
    if !key.is_absolute() {
        return Err(Error::NoIdentity);
    }
    if !key.is_file() {
        return Err(Error::KeyMissing(key));
    }
    // The file at the saved path must still be this device's key: a replaced
    // file or retargeted link would otherwise reach another VM.
    if keys::fingerprint(&key, &cancelled)? != id {
        return Err(Error::KeyChanged(key));
    }
    ssh::command(&key, &ssh::known_hosts(dir), false)
}

/// Reuse the saved device's identity and host-key policy for background scripts.
pub fn command(account: &str, id: &str, stop: &AtomicBool) -> Result<Command> {
    saved_command(&ssh::state_dir()?, account, id, stop)
}

/// Reach the Railway device `id`, whose key is at `account`, for
/// `cloud::connect`.
pub fn connect(account: &str, id: &str, session: &str, stop: &AtomicBool) -> io::Result<Transport> {
    match ssh::state_dir().and_then(|dir| connect_saved(&dir, account, id, session, stop)) {
        Ok(bridge) => Ok(bridge.into()),
        Err(Error::Bridge(error)) => Err(io::Error::new(error.kind(), error)),
        Err(error) => Err(io::Error::other(error)),
    }
}

// Tests drive `ssh`, `ssh-keygen`, and `/bin/sh`; cloud devices are Unix-only.
#[cfg(all(test, unix))]
mod tests;
