//! The herdr SSH key on this machine, the public keys machines trade, and
//! this machine's `authorized_keys`.
use super::Error;
use crate::ssh::identity;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
    process::Stdio,
};

/// A public key file is one short line; anything longer is not one.
const PUBLIC_KEY_LIMIT: u64 = 4096;

/// An `ssh-ed25519` public key without its comment, the form a key is
/// recognised by in `authorized_keys` whatever comment it was added with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublicKey(String);

impl PublicKey {
    /// The key of a `.pub` line. Only the keys machine sync creates are
    /// accepted, so nothing but base64 ever reaches a remote script.
    pub(crate) fn parse(line: &str) -> Result<Self, Error> {
        let mut words = line.split_whitespace();
        let (Some(kind), Some(data)) = (words.next(), words.next()) else {
            return Err(Error::PublicKey);
        };
        let base64 = |byte: u8| byte.is_ascii_alphanumeric() || b"+/=".contains(&byte);
        if kind != "ssh-ed25519" || data.len() > 128 || !data.bytes().all(base64) {
            return Err(Error::PublicKey);
        }
        Ok(Self(format!("{kind} {data}")))
    }

    /// The key as `authorized_keys` matches it.
    pub(crate) fn body(&self) -> &str {
        &self.0
    }

    /// The `authorized_keys` line, its comment naming the machine it is for,
    /// so the user can find and remove it.
    pub(crate) fn line(&self, label: &str) -> String {
        let name: String = label
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
            .take(64)
            .collect();
        let name = if name.is_empty() { "machine" } else { &name };
        format!("{} herdr-sync:{name}", self.0)
    }
}

/// This machine's herdr key, created first when it has none.
/// Blocks on `ssh-keygen`: call it from a background worker.
pub(crate) fn ensure_local_key() -> Result<PublicKey, Error> {
    let path = identity::key_path().ok_or(Error::Home)?;
    if !path.exists() {
        if let Some(directory) = path.parent() {
            create_ssh_directory(directory).map_err(Error::KeyRead)?;
        }
        let status = crate::environment::command("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "herdr-sync", "-f"])
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(Error::KeygenSpawn)?;
        if !status.success() {
            return Err(Error::Keygen(status));
        }
    }
    let mut public = path.into_os_string();
    public.push(".pub");
    let mut text = String::new();
    fs::File::open(&public)
        .map_err(Error::KeyRead)?
        .take(PUBLIC_KEY_LIMIT)
        .read_to_string(&mut text)
        .map_err(Error::KeyRead)?;
    PublicKey::parse(text.lines().next().unwrap_or_default())
}

/// Adds each of `keys` that this machine's `authorized_keys` lacks, labelled
/// with the machine it belongs to, and returns how many were added.
pub(crate) fn authorize_locally(keys: &[(PublicKey, String)]) -> Result<usize, Error> {
    let home = crate::local_path::home_dir().ok_or(Error::Home)?;
    let directory = home.join(".ssh");
    let path = directory.join("authorized_keys");
    let wrap = |source| Error::AuthorizedKeys {
        path: path.clone(),
        source,
    };
    let existing = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(wrap(error)),
    };
    let (append, added) = missing_lines(&existing, keys);
    if added == 0 {
        return Ok(0);
    }
    create_ssh_directory(&directory).map_err(wrap)?;
    let mut options = fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&path).map_err(wrap)?;
    file.write_all(append.as_bytes()).map_err(wrap)?;
    file.sync_all().map_err(wrap)?;
    Ok(added)
}

/// The text to append to `existing` for the keys it lacks, starting a new
/// line first when its last line has no end, and how many keys that is.
pub(crate) fn missing_lines(existing: &str, keys: &[(PublicKey, String)]) -> (String, usize) {
    let mut append = String::new();
    let mut added = 0;
    for (key, label) in keys {
        let present = existing.contains(key.body()) || append.contains(key.body());
        if present {
            continue;
        }
        append.push_str(&key.line(label));
        append.push('\n');
        added += 1;
    }
    if added > 0 && !existing.is_empty() && !existing.ends_with('\n') {
        append.insert(0, '\n');
    }
    (append, added)
}

/// `~/.ssh`, private as sshd requires, unless it exists already.
fn create_ssh_directory(directory: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(directory)
}
