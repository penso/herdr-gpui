//! The `ssh` command that reaches a Railway free VM: `ssh railway.new` with
//! one explicit identity. `-F /dev/null` skips the user's and the system's ssh
//! config, so no alias, proxy, or forward of theirs applies. An agent can sign
//! for the chosen key, whose public half `keys::fingerprint` checks before
//! setup or reconnect builds this command.
//!
//! Railway documents no host key for `railway.new`, so the first one seen is
//! pinned in a GUI-owned `known_hosts` and a later change is refused.

use super::{Error, Result};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

const HOST: &str = "railway.new";

/// Everything but the two paths. Command-line options win over any config.
const POLICY: [&str; 21] = [
    "AddressFamily=inet",
    "BatchMode=yes",
    "NumberOfPasswordPrompts=0",
    "PreferredAuthentications=publickey",
    "PasswordAuthentication=no",
    "IdentitiesOnly=yes",
    "StrictHostKeyChecking=accept-new",
    "VerifyHostKeyDNS=no",
    "GlobalKnownHostsFile=/dev/null",
    "ConnectTimeout=15",
    "ConnectionAttempts=1",
    "ServerAliveInterval=15",
    "ServerAliveCountMax=4",
    "ForwardAgent=no",
    "ForwardX11=no",
    "ClearAllForwardings=yes",
    "ControlMaster=no",
    "ControlPath=none",
    "ProxyCommand=none",
    "PermitLocalCommand=no",
    "LogLevel=ERROR",
];

/// A path inside a quoted `-o` value: `%` starts a token there, and nothing may
/// end the value or the quotes.
fn quoted(path: &Path) -> Result<String> {
    let text = path.to_str().ok_or(Error::KeyPath)?;
    if !path.is_absolute() || text.contains(['"', '\n', '\r', '\0']) {
        return Err(Error::KeyPath);
    }
    Ok(format!("\"{}\"", text.replace('%', "%%")))
}

/// A fresh `ssh` command presenting `identity`; append the remote command. It
/// is headless (`-T`), or with `terminal` asks for a terminal on the VM even
/// though this process has none, for the one connection that creates it. Only
/// `first_use` may run that, on a pseudo-terminal it provides.
pub(super) fn command(identity: &Path, known_hosts: &Path, terminal: bool) -> Result<Command> {
    let identity = format!("IdentityFile={}", quoted(identity)?);
    let known_hosts_option = format!("UserKnownHostsFile={}", quoted(known_hosts)?);
    if let Some(parent) = known_hosts.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Store {
            path: parent.to_owned(),
            source,
        })?;
    }
    let mut command = base();
    command.args(["-F", "/dev/null", if terminal { "-tt" } else { "-T" }]);
    for option in POLICY
        .into_iter()
        .chain([identity.as_str(), known_hosts_option.as_str()])
    {
        command.arg("-o").arg(option);
    }
    command.args(["--", HOST]);
    Ok(command)
}

/// The command every `ssh` run starts from. Tests on one thread may stand in
/// a script that checks what `ssh` would be handed.
fn base() -> Command {
    #[cfg(test)]
    if let Some(command) = stand_in::base() {
        return command;
    }
    sheprd_client::new_command("ssh")
}

/// A stand-in for `ssh` that lets the setup and reconnect flows run end to end
/// without a network.
#[cfg(all(test, unix))]
pub(super) mod stand_in;
#[cfg(all(test, not(unix)))]
mod stand_in {
    pub(super) fn base() -> Option<std::process::Command> {
        None
    }
}

/// The directory Railway keeps its own files in.
pub(super) fn state_dir() -> Result<PathBuf> {
    #[cfg(all(test, unix))]
    if let Some(dir) = stand_in::state_dir() {
        return Ok(dir);
    }
    sheprd_client::local_path::state_dir().ok_or(Error::Cloud(crate::cloud::Error::StateDirectory))
}

/// The GUI's own record of the host key `railway.new` presented.
pub fn known_hosts(dir: &Path) -> PathBuf {
    dir.join("railway_known_hosts")
}

// Tests drive `ssh`, `ssh-keygen`, and `/bin/sh`; cloud devices are Unix-only.
#[cfg(all(test, unix))]
mod tests;
