//! Trusting an unknown SSH host key after the user approved its fingerprint.
//!
//! [`scan_host_key`] lets `ssh` itself record the key a host offers, into a
//! private scratch file listed ahead of the user's own known hosts files.
//! `StrictHostKeyChecking=accept-new` adds only keys no listed file knows, so
//! a changed key is refused exactly as a normal connection refuses it, and
//! `ssh` applies the user's `HostKeyAlias`, port, and hashing rules to the
//! entry. Nothing reaches the user's files until [`trust_host_key`] appends
//! those exact entries, so the approved fingerprint is the one saved.
use super::{Error, SshFailure};
use crate::Result;
use crate::{StorageOperation, catalog::validate_target};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::{
    fs::{DirBuilder, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    process::Stdio,
    time::Duration,
};

/// Upper bound on the entries `ssh` records for one host, normally one line
/// (two when `CheckHostIP` also records the address).
const ENTRIES_LIMIT: u64 = 16 * 1024;

/// What a host's key check found, learned without authenticating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyScan {
    /// The host offered a key no known hosts file lists yet.
    New(NewHostKey),
    /// The key is already trusted, so host verification passed.
    Trusted,
    /// `ssh` failed for this reason. [`SshFailure::HostKey`] here means the
    /// host offered a key that differs from the one already trusted, which
    /// only the user can resolve.
    Refused(SshFailure),
}

/// A host key the user has not trusted yet, with the known hosts entries
/// `ssh` would write for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHostKey {
    pub fingerprints: Vec<HostKeyFingerprint>,
    entries: String,
    known_hosts: PathBuf,
    /// The name the host's keys are stored under, and every known hosts file
    /// checked for it, checked again right before the entries are saved.
    name: String,
    checked: Vec<PathBuf>,
}

/// One key as `ssh-keygen -l` describes it, for the user to compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKeyFingerprint {
    /// Such as `ED25519` or `RSA`.
    pub kind: String,
    pub bits: u32,
    /// `SHA256:` followed by the base64 digest.
    pub fingerprint: String,
}

impl NewHostKey {
    /// The file the entries are appended to.
    pub fn known_hosts(&self) -> &Path {
        &self.known_hosts
    }
}

/// Connects to `target` without authenticating, only to learn its host key.
/// Every authentication method is off, so no credential is offered and no
/// remote command runs. Blocks on SSH: call it from a background thread.
pub fn scan_host_key(target: &str) -> Result<HostKeyScan> {
    validate_target(target)?;
    let known = known_hosts(target)?;
    let files = &known.user;
    let scratch = tempfile::Builder::new()
        .prefix("herdr-host-key-")
        .tempdir()?;
    let recorded = scratch.path().join("known_hosts");
    std::fs::File::create(&recorded)?;
    let mut command = crate::environment::command("ssh");
    command
        .args(scan_options(&recorded, files))
        .args(super::POLICY)
        .args(["-o", "ClearAllForwardings=yes", "--", target, "true"])
        .stderr(Stdio::piped());
    // Stderr is read only to classify a failure, as for the probe.
    let (status, _, stderr) =
        super::run_with_stderr(&mut command, Duration::from_secs(20), || false)?;
    let mut entries = String::new();
    std::fs::File::open(&recorded)?
        .take(ENTRIES_LIMIT + 1)
        .read_to_string(&mut entries)
        .map_err(|_| crate::Error::RemoteOutput)?;
    if entries.len() as u64 > ENTRIES_LIMIT {
        return Err(Error::OutputLimit.into());
    }
    if !entries.trim().is_empty() {
        // `accept-new` treats a host with a known key of another type as
        // changed in current OpenSSH; older releases may record the new type.
        // Either way, a host that already has a trusted key is never offered.
        if lists_host(&known.name, known.files())? {
            tracing::warn!(category = "host_key", "host offered a key of another type");
            return Ok(HostKeyScan::Refused(SshFailure::HostKey));
        }
        let fingerprints = fingerprints(&recorded)?;
        tracing::info!(
            category = "host_key",
            keys = fingerprints.len(),
            "host offered a new key"
        );
        return Ok(HostKeyScan::New(NewHostKey {
            fingerprints,
            entries: valid_entries(&entries)?,
            known_hosts: files[0].clone(),
            name: known.name.clone(),
            checked: known.files().cloned().collect(),
        }));
    }
    Ok(match status.code() {
        Some(255) => match SshFailure::classify(status.code(), &stderr) {
            // Authentication is refused only after the host was verified.
            SshFailure::Auth => HostKeyScan::Trusted,
            failure => {
                tracing::warn!(category = "host_key", ?failure, "host key check failed");
                HostKeyScan::Refused(failure)
            }
        },
        _ => HostKeyScan::Trusted,
    })
}

/// Options that come before [`super::POLICY`], so they win over it: record a
/// new key into `recorded`, check the user's own files too, and never offer a
/// credential or touch a shared connection. Nothing is forwarded either, since
/// the host is not verified: the user's config may forward the agent to it.
fn scan_options(recorded: &Path, files: &[PathBuf]) -> Vec<String> {
    let known: Vec<String> = std::iter::once(recorded)
        .chain(files.iter().map(PathBuf::as_path))
        .map(|path| format!("\"{}\"", option_path(path)))
        .collect();
    [
        format!("UserKnownHostsFile={}", known.join(" ")),
        "StrictHostKeyChecking=accept-new".into(),
        "UpdateHostKeys=no".into(),
        "PubkeyAuthentication=no".into(),
        "PasswordAuthentication=no".into(),
        "KbdInteractiveAuthentication=no".into(),
        "GSSAPIAuthentication=no".into(),
        "HostbasedAuthentication=no".into(),
        "ControlPath=none".into(),
        "ForwardAgent=no".into(),
        "ForwardX11Trusted=no".into(),
        "PermitLocalCommand=no".into(),
        "RemoteCommand=none".into(),
        "Tunnel=no".into(),
    ]
    .into_iter()
    .flat_map(|option| ["-o".into(), option])
    .collect()
}

/// `path` for a quoted list in an `ssh` option. OpenSSH reads a backslash in
/// one as an escape before a quote or another backslash, so on Windows, whose
/// OpenSSH takes either separator, the path is written with forward slashes.
fn option_path(path: &Path) -> String {
    let text = path.display().to_string();
    if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text
    }
}

/// What `ssh -G` resolves for a target's host keys.
#[derive(Debug, PartialEq, Eq)]
struct KnownHosts {
    /// The user files, first one first: that is where `ssh` itself saves a
    /// key it was told to accept.
    user: Vec<PathBuf>,
    global: Vec<PathBuf>,
    /// The name keys are stored under: `HostKeyAlias`, or the host name,
    /// with the port in brackets when it is not 22.
    name: String,
}

impl KnownHosts {
    fn files(&self) -> impl Iterator<Item = &PathBuf> {
        self.user.iter().chain(&self.global)
    }
}

/// Whether any of `files` already lists a key under `name`; `ssh-keygen -F`
/// also matches hashed entries. Only a missing file is skipped: one that
/// cannot be read fails the check rather than passing it.
fn lists_host<'a>(name: &str, files: impl IntoIterator<Item = &'a PathBuf>) -> Result<bool> {
    for file in files {
        match std::fs::metadata(file) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(crate::Error::storage(
                    StorageOperation::Metadata,
                    file,
                    error,
                ));
            }
            Ok(metadata) if !metadata.is_file() => {
                return Err(crate::Error::storage(
                    StorageOperation::Metadata,
                    file,
                    crate::Error::StorageDestinationNotFile,
                ));
            }
            Ok(_) => {}
        }
        let mut command = crate::environment::command("ssh-keygen");
        command.args(["-F", name, "-f"]).arg(file);
        let (status, _) = super::run(&mut command, Duration::from_secs(5), || false)?;
        match status.code() {
            Some(0) => return Ok(true),
            Some(1) => {}
            _ => return Err(crate::Error::RemoteCommand(status)),
        }
    }
    Ok(false)
}

fn known_hosts(target: &str) -> Result<KnownHosts> {
    let mut command = crate::environment::command("ssh");
    command.args(["-G", "--", target]);
    let (status, output) = super::run(&mut command, Duration::from_secs(5), || false)?;
    if !status.success() {
        return Err(crate::Error::RemoteCommand(status));
    }
    parse_known_hosts(&output).ok_or(Error::KnownHostsUnavailable.into())
}

/// `None` unless every user file is an absolute path without quotes, and any
/// space in one is accounted for (see [`split_paths`]), since `ssh -G` prints
/// paths unquoted.
fn parse_known_hosts(output: &[u8]) -> Option<KnownHosts> {
    let text = std::str::from_utf8(output).ok()?;
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(' '))
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "none")
    };
    let paths = |value: Option<&str>| -> Option<Vec<PathBuf>> {
        let files: Vec<PathBuf> = split_paths(value?)
            .iter()
            .map(|file| expand_path(file))
            .collect();
        files
            .iter()
            .all(|file| {
                file.is_absolute()
                    && !file.to_string_lossy().contains(['"', '\''])
                    && spaces_in_directories(file, Path::is_dir)
            })
            .then_some(files)
    };
    let user = paths(value("userknownhostsfile")).filter(|files| !files.is_empty())?;
    // Global files are only read; an unusable list just checks fewer files.
    let global = paths(value("globalknownhostsfile")).unwrap_or_default();
    let host = match value("hostkeyalias") {
        Some(alias) => alias.to_owned(),
        None => value("hostname")?.to_ascii_lowercase(),
    };
    let name = match value("port")?.parse::<u16>().ok()? {
        22 => host,
        port => format!("[{host}]:{port}"),
    };
    if name.starts_with('-') || name.chars().any(char::is_control) {
        return None;
    }
    Some(KnownHosts { user, global, name })
}

/// The files in one `ssh -G` list, which joins them with single spaces and
/// quotes none. On Unix a path with a space cannot be told from two paths, so
/// it stays split and the caller rejects the fragment. On Windows every
/// absolute path starts with a drive or `\\` and no file name holds a `:`,
/// so a word that starts neither continues the path before it, as in a
/// profile such as `C:\Users\Jane Doe`. A relative file listed after an
/// absolute one joins it the same way; [`spaces_in_directories`] refuses that
/// join rather than let it name a file `ssh` never reads.
fn split_paths(list: &str) -> Vec<String> {
    if !cfg!(windows) {
        return list.split_whitespace().map(str::to_owned).collect();
    }
    let mut files: Vec<String> = Vec::new();
    for word in list.split(' ') {
        let starts = word.starts_with("__PROGRAMDATA__") || Path::new(word).is_absolute();
        match files.last_mut() {
            Some(file) if !starts => {
                file.push(' ');
                file.push_str(word);
            }
            _ => files.push(word.to_owned()),
        }
    }
    files
}

/// Whether every space in `file` lies in a directory that exists. A space
/// in the file name itself, or in a directory that `is_dir` does not find,
/// may as well separate two listed files, so such a path is refused: trusting
/// a key there would write it where `ssh` does not look.
fn spaces_in_directories(file: &Path, is_dir: impl Fn(&Path) -> bool) -> bool {
    let components: Vec<_> = file.components().collect();
    let mut prefix = PathBuf::new();
    for (index, component) in components.iter().enumerate() {
        prefix.push(component);
        let spaced = component.as_os_str().to_string_lossy().contains(' ');
        if spaced && (index + 1 == components.len() || !is_dir(&prefix)) {
            return false;
        }
    }
    true
}

/// A path as `ssh -G` prints it. Windows OpenSSH prints its system-wide
/// directory as `__PROGRAMDATA__`, which it expands only when it opens a file.
fn expand_path(path: &str) -> PathBuf {
    if cfg!(windows)
        && let Some(rest) = path.strip_prefix("__PROGRAMDATA__")
        && let Some(data) = std::env::var_os("ProgramData")
    {
        let mut expanded = PathBuf::from(data).into_os_string();
        expanded.push(rest);
        return expanded.into();
    }
    PathBuf::from(path)
}

/// Describes each key in `file`, one per distinct fingerprint.
fn fingerprints(file: &Path) -> Result<Vec<HostKeyFingerprint>> {
    let mut command = crate::environment::command("ssh-keygen");
    command.args(["-l", "-E", "sha256", "-f"]).arg(file);
    let (status, output) = super::run(&mut command, Duration::from_secs(5), || false)?;
    if !status.success() {
        return Err(crate::Error::RemoteCommand(status));
    }
    let mut keys: Vec<HostKeyFingerprint> = Vec::new();
    for key in std::str::from_utf8(&output)
        .map_err(|_| crate::Error::RemoteOutput)?
        .lines()
        .map(parse_fingerprint)
    {
        let key = key.ok_or(crate::Error::RemoteOutput)?;
        if !keys
            .iter()
            .any(|known| known.fingerprint == key.fingerprint)
        {
            keys.push(key);
        }
    }
    if keys.is_empty() {
        return Err(crate::Error::RemoteOutput);
    }
    Ok(keys)
}

/// One `ssh-keygen -l` line: `256 SHA256:<digest> <host> (ED25519)`.
fn parse_fingerprint(line: &str) -> Option<HostKeyFingerprint> {
    let mut words = line.split_whitespace();
    let bits = words.next()?.parse().ok()?;
    let fingerprint = words.next()?;
    let digest = fingerprint.strip_prefix("SHA256:")?;
    if digest.is_empty()
        || !digest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
    {
        return None;
    }
    let kind = words.last()?.strip_prefix('(')?.strip_suffix(')')?;
    if kind.is_empty() || !kind.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    Some(HostKeyFingerprint {
        kind: kind.to_owned(),
        bits,
        fingerprint: fingerprint.to_owned(),
    })
}

/// Known hosts lines as `ssh` wrote them, refused if any carries a control
/// character, so appending them can never add something else.
fn valid_entries(entries: &str) -> Result<String> {
    let mut valid = String::with_capacity(entries.len());
    for line in entries.lines().filter(|line| !line.trim().is_empty()) {
        if line.chars().any(char::is_control) {
            return Err(crate::Error::RemoteOutput);
        }
        valid.push_str(line);
        valid.push('\n');
    }
    Ok(valid)
}

/// Appends the approved entries to the user's known hosts file, creating it
/// and its directory privately when missing. Blocks on disk: call it from a
/// background thread.
pub fn trust_host_key(key: &NewHostKey) -> Result<()> {
    // The files may have changed since the key was shown: a key trusted for
    // the host meanwhile makes this one a replacement, which is never saved.
    if lists_host(&key.name, &key.checked)? {
        tracing::warn!(
            category = "host_key",
            "host gained a trusted key before approval"
        );
        return Err(Error::Refused(SshFailure::HostKey).into());
    }
    let path = key.known_hosts.as_path();
    if let Some(parent) = path.parent() {
        let mut builder = DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(parent).map_err(|error| {
            crate::Error::storage(StorageOperation::CreateDirectory, parent, error)
        })?;
    }
    // Windows has no mode bits: the file inherits its directory's ACL, as
    // one `ssh` itself creates there does.
    let mut options = OpenOptions::new();
    options.read(true).append(true).create(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .map_err(|error| crate::Error::storage(StorageOperation::Open, path, error))?;
    let write = |error| crate::Error::storage(StorageOperation::Write, path, error);
    // A last line without a newline would otherwise swallow the first entry.
    let mut last = [0];
    let unterminated = file.seek(SeekFrom::End(0)).map_err(write)? > 0
        && file.seek(SeekFrom::End(-1)).is_ok()
        && file.read_exact(&mut last).is_ok()
        && last[0] != b'\n';
    let mut text = String::with_capacity(key.entries.len() + 1);
    if unterminated {
        text.push('\n');
    }
    text.push_str(&key.entries);
    file.write_all(text.as_bytes()).map_err(write)?;
    file.sync_all()
        .map_err(|error| crate::Error::storage(StorageOperation::Sync, path, error))?;
    tracing::info!(category = "host_key", "trusted a new host key");
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Test fixtures only.
mod tests;
