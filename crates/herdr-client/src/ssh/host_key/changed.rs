//! Replacing a host key that differs from the one trusted for the host, after
//! the user compared both fingerprints and chose the new one.
//!
//! [`scan_changed_host_key`] lets `ssh` record the key the host offers into a
//! scratch file, checked against nothing else so `accept-new` takes it, and
//! reads the fingerprints the user's files trust for the host. Nothing is
//! written until [`replace_host_key`] finds those same keys still trusted,
//! removes them with `ssh-keygen -R` (which keeps a `.old` copy of each file),
//! and appends the recorded entries. A key trusted system-wide, or pinned by a
//! `@cert-authority` or `@revoked` line, is never replaced here.

use super::{
    Error, HostKeyFingerprint, KnownHosts, SshFailure, append_entries, fingerprints, known_hosts,
    lists_host, read_entries, scan_options, valid_entries,
};
use crate::Result;
use crate::catalog::validate_target;
use std::path::{Path, PathBuf};
use std::{process::Stdio, time::Duration};

/// A host whose offered key differs from the one already trusted for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedHostKey {
    /// The keys the host offers now.
    pub fingerprints: Vec<HostKeyFingerprint>,
    /// The keys the user's known hosts files trust for the host.
    pub trusted: Vec<HostKeyFingerprint>,
    entries: String,
    known_hosts: PathBuf,
    name: String,
    /// Every user file checked for the host, and every global one, which must
    /// still not list it when the key is replaced.
    user: Vec<PathBuf>,
    global: Vec<PathBuf>,
}

impl ChangedHostKey {
    /// The file the new entries are appended to.
    pub fn known_hosts(&self) -> &Path {
        &self.known_hosts
    }
}

/// Learns the key `target` offers and the keys trusted for it, without
/// authenticating. Blocks on SSH: call it from a background thread.
pub fn scan_changed_host_key(target: &str) -> Result<ChangedHostKey> {
    validate_target(target)?;
    let known = known_hosts(target)?;
    let trusted = trusted_keys(&known)?;
    let scratch = tempfile::Builder::new()
        .prefix("herdr-host-key-")
        .tempdir()?;
    let recorded = scratch.path().join("known_hosts");
    std::fs::File::create(&recorded)?;
    let mut command = crate::environment::command("ssh");
    command
        .args(scan_options(&recorded, &[]))
        .args(["-o", "GlobalKnownHostsFile=none"])
        .args(super::super::POLICY)
        .args(["-o", "ClearAllForwardings=yes", "--", target, "true"])
        .stderr(Stdio::piped());
    let (status, _, stderr) =
        super::super::run_with_stderr(&mut command, Duration::from_secs(20), || false)?;
    let entries = read_entries(&recorded)?;
    if entries.trim().is_empty() {
        let failure = SshFailure::classify(status.code(), &stderr);
        tracing::warn!(
            category = "host_key",
            ?failure,
            "changed host key not fetched"
        );
        return Err(Error::Refused(failure).into());
    }
    let fingerprints = fingerprints(&recorded)?;
    tracing::info!(
        category = "host_key",
        keys = fingerprints.len(),
        "host offered a changed key"
    );
    Ok(ChangedHostKey {
        fingerprints,
        trusted,
        entries: valid_entries(&entries)?,
        known_hosts: known.user[0].clone(),
        name: known.name,
        user: known.user,
        global: known.global,
    })
}

/// The keys the user's files trust for the host. A host listed in a global
/// file, or by a marker line, cannot be changed by replacing user entries.
fn trusted_keys(known: &KnownHosts) -> Result<Vec<HostKeyFingerprint>> {
    if lists_host(&known.name, &known.global)? {
        return Err(Error::KeyNotReplaceable.into());
    }
    let lines = listed_lines(&known.name, &known.user)?;
    if lines.is_empty() {
        // Nothing the user's files trust: not a changed key after all.
        return Err(Error::Refused(SshFailure::HostKey).into());
    }
    if lines.iter().any(|line| line.starts_with('@')) {
        return Err(Error::KeyNotReplaceable.into());
    }
    // A closed file in a scratch directory: Windows keeps a `NamedTempFile`
    // open without sharing, so `ssh-keygen` could not read it.
    let scratch = tempfile::Builder::new()
        .prefix("herdr-host-key-")
        .tempdir()?;
    let file = scratch.path().join("trusted");
    std::fs::write(&file, lines.join("\n") + "\n")?;
    fingerprints(&file)
}

/// Every known hosts line under `name` in `files`, as `ssh-keygen -F` finds
/// them, hashed entries included. Missing files are skipped.
fn listed_lines(name: &str, files: &[PathBuf]) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for file in files.iter().filter(|file| file.exists()) {
        if !lists_host(name, [file])? {
            continue;
        }
        let mut command = crate::environment::command("ssh-keygen");
        command.args(["-F", name, "-f"]).arg(file);
        let (status, output) = super::super::run(&mut command, Duration::from_secs(5), || false)?;
        if !status.success() {
            return Err(crate::Error::RemoteCommand(status));
        }
        let text = std::str::from_utf8(&output).map_err(|_| crate::Error::RemoteOutput)?;
        lines.extend(
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(str::to_owned),
        );
    }
    Ok(lines)
}

/// Replaces the trusted keys with the approved ones. Refused when the files
/// trust anything else for the host by now. Blocks on disk and `ssh-keygen`:
/// call it from a background thread.
pub fn replace_host_key(key: &ChangedHostKey) -> Result<()> {
    let known = KnownHosts {
        user: key.user.clone(),
        global: key.global.clone(),
        name: key.name.clone(),
    };
    if trusted_keys(&known)? != key.trusted {
        tracing::warn!(
            category = "host_key",
            "trusted host key changed before approval"
        );
        return Err(Error::Refused(SshFailure::HostKey).into());
    }
    for file in key.user.iter().filter(|file| file.exists()) {
        if !lists_host(&key.name, [file])? {
            continue;
        }
        let mut command = crate::environment::command("ssh-keygen");
        command.args(["-R", &key.name, "-f"]).arg(file);
        let (status, _) = super::super::run(&mut command, Duration::from_secs(5), || false)?;
        if !status.success() {
            return Err(crate::Error::RemoteCommand(status));
        }
    }
    append_entries(&key.known_hosts, &key.entries)?;
    tracing::info!(category = "host_key", "replaced a changed host key");
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Test fixtures only.
mod tests;
