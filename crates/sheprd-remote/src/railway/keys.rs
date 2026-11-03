//! SSH keys for Railway devices, through the system's `ssh-keygen`: reading a
//! key's fingerprint (which names its device) and generating a new one. The
//! GUI never parses or holds private key bytes. Runs are bounded, take no
//! terminal input, and observe cancellation.

use super::{Error, Result};
use crate::cloud::{self, install};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(30);
const COMMENT: &str = "herdr-gpui railway";
/// Herdr's own folder for keys it generates when there is no `~/.ssh`.
const KEYS: &str = "railway-keys";

/// Where the public half of the key at `path` is: `<path>.pub`. Appended to
/// the whole name, since key names may hold dots.
pub(super) fn public_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".pub");
    PathBuf::from(name)
}

/// Run an `ssh-keygen` command with `input` on its stdin, bounded, killed on
/// cancellation, through the same supervisor the installer uses.
fn run(
    mut command: Command,
    input: &str,
    cancelled: &impl Fn() -> bool,
) -> Result<install::Finished> {
    use io::Write;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(Error::KeygenMissing)?;
    // A short public key fits the pipe buffer; closing stdin ends the input.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let output = Arc::new(Mutex::new(Vec::new()));
    let reader = child
        .stdout
        .take()
        .map(|pipe| install::drain(pipe, output.clone()));
    match install::supervise(child, [reader], &output, TIMEOUT, cancelled) {
        Err(cloud::Error::InstallTimeout) => Err(Error::KeygenTimeout),
        finished => Ok(finished?),
    }
}

/// The device ID in `ssh-keygen -l` output such as
/// `256 SHA256:abc… comment (ED25519)`: `key-` and the first 16 letters and
/// digits of the digest.
fn id_from(listing: &str) -> Option<String> {
    let word = listing.lines().next()?.split_whitespace().nth(1)?;
    let id: String = word
        .strip_prefix("SHA256:")?
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(16)
        .collect();
    (id.len() == 16).then(|| format!("key-{id}"))
}

fn digest(listing: &install::Finished) -> Option<&str> {
    (listing.code == Some(0))
        .then(|| listing.output.split_whitespace().nth(1))
        .flatten()
        .filter(|word| word.starts_with("SHA256:"))
}

/// The device ID of the key at `path`, taken from the private key itself.
/// Reading it through `ssh-keygen -l -f <key>` would trust a `.pub` beside it,
/// which stays behind when the private file is replaced. Only a
/// passphrase-protected key, which cannot be read unattended, falls back to its
/// `.pub`. When the private key can be read, its public half must agree:
/// OpenSSH can otherwise authenticate with that half's key from the agent.
pub(super) fn fingerprint(path: &Path, cancelled: &impl Fn() -> bool) -> Result<String> {
    let list = |source: &std::ffi::OsStr, input: &str| {
        let mut command = sheprd_client::new_command("ssh-keygen");
        command.args(["-l", "-E", "sha256", "-f"]).arg(source);
        run(command, input, cancelled)
    };
    let mut derive = sheprd_client::new_command("ssh-keygen");
    derive.args(["-y", "-P", ""]).arg("-f").arg(path);
    let listing = match run(derive, "", cancelled)? {
        install::Finished {
            code: Some(0),
            output: public,
        } => {
            let derived = list("-".as_ref(), &public)?;
            let public = public_path(path);
            match fs::metadata(&public) {
                Ok(_) => {
                    let offered = list(public.as_os_str(), "")?;
                    // Compare the full fingerprint, not the shortened device ID.
                    if digest(&derived).is_none() || digest(&derived) != digest(&offered) {
                        return Err(Error::PublicKeyMismatch(path.to_owned()));
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(Error::KeyFile {
                        path: public,
                        source,
                    });
                }
            }
            derived
        }
        _ => list(public_path(path).as_os_str(), "")?,
    };
    (listing.code == Some(0))
        .then(|| id_from(&listing.output))
        .flatten()
        .ok_or_else(|| Error::UnusableKey(path.to_owned()))
}

/// A key chosen from the user's own files: an absolute path to a regular
/// file, short enough to be saved with the device. Symlinks are followed by
/// `ssh` itself, so the path is kept as given rather than resolved, and
/// nothing is read here but the fingerprint.
pub(super) fn existing(path: &Path) -> Result<PathBuf> {
    let savable = path
        .to_str()
        .is_some_and(|text| text.len() <= 2048 && !text.chars().any(char::is_control));
    if !path.is_absolute() || !savable {
        return Err(Error::KeyPath);
    }
    let metadata = fs::metadata(path).map_err(|source| Error::KeyFile {
        path: path.to_owned(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(Error::KeyPath);
    }
    Ok(path.to_owned())
}

#[cfg(unix)]
fn private_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

#[cfg(not(unix))]
fn private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
}

/// The rule for key names, stated once for the UI and the backend alike.
pub const NAME_RULE: &str = "Use 1 to 64 letters, digits, dots, underscores or hyphens, not starting with a dot or hyphen and not ending in .pub.";

/// A key name that is one safe file name: no separators, traversal, hidden or
/// option-like names, and nothing that collides with a public key's name.
pub fn valid_name(name: &str) -> Result<()> {
    let plain = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['.', '-'])
        && !name.ends_with(".pub")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    plain.then_some(()).ok_or(Error::KeyName)
}

/// Where new keys go: the user's `~/.ssh` when it is a directory, else
/// Herdr's own private folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyDirectory {
    pub path: PathBuf,
    /// Whether this is the user's `~/.ssh`, whose keys are theirs to keep.
    pub user: bool,
}

pub fn key_directory(home: Option<&Path>, state: &Path) -> KeyDirectory {
    match home
        .map(|home| home.join(".ssh"))
        .filter(|ssh| ssh.is_dir())
    {
        Some(path) => KeyDirectory { path, user: true },
        None => KeyDirectory {
            path: state.join(KEYS),
            user: false,
        },
    }
}

fn exists(path: &Path) -> bool {
    // A symlink, even a dangling one, is a name already taken.
    fs::symlink_metadata(path).is_ok()
}

/// Generate an ed25519 key named `name` in `directory`, with no passphrase
/// (the app must use it unattended on every reconnect). Never replaces
/// anything: the pair is made inside a scratch directory this call owns, then
/// hard-linked to its final names, which fails if either is taken. Only that
/// scratch directory is ever removed. `ssh-keygen` creates the private half
/// readable only by this user; Herdr's own folder is created only this user
/// can open, and `~/.ssh` is left as the user has it.
pub(super) fn generate(
    directory: &KeyDirectory,
    name: &str,
    cancelled: &impl Fn() -> bool,
) -> Result<PathBuf> {
    valid_name(name)?;
    let store = |path: &Path| {
        let path = path.to_owned();
        move |source| Error::Store { path, source }
    };
    if !directory.user {
        private_directory(&directory.path).map_err(store(&directory.path))?;
    }
    let target = directory.path.join(name);
    let public_target = public_path(&target);
    if exists(&target) || exists(&public_target) {
        return Err(Error::KeyExists(target));
    }
    // Beside the final names, so linking never crosses a file system.
    let scratch = tempfile::Builder::new()
        .prefix(".herdr-new-")
        .tempdir_in(&directory.path)
        .map_err(store(&directory.path))?;
    let key = scratch.path().join("key");
    let mut command = sheprd_client::new_command("ssh-keygen");
    command
        .args(["-q", "-t", "ed25519", "-N", "", "-C", COMMENT])
        .arg("-f")
        .arg(&key);
    let made = run(command, "", cancelled)?;
    if made.code != Some(0) {
        return Err(Error::Keygen(made.code));
    }
    place(&key, &target)?;
    if let Err(error) = place(&public_path(&key), &public_target) {
        // The private half was linked just above, by this call.
        let _ = fs::remove_file(&target);
        return Err(error);
    }
    Ok(target)
}

/// Give `from` the name `to` without replacing whatever has it: a hard link
/// fails if `to` exists, atomically. There is no fallback; a file system
/// without hard links makes key generation fail rather than risk an overwrite.
fn place(from: &Path, to: &Path) -> Result<()> {
    fs::hard_link(from, to).map_err(|source| {
        if source.kind() == io::ErrorKind::AlreadyExists {
            Error::KeyExists(to.to_owned())
        } else {
            Error::Store {
                path: to.to_owned(),
                source,
            }
        }
    })
}

// Tests drive `ssh`, `ssh-keygen`, and `/bin/sh`; cloud devices are Unix-only.
#[cfg(all(test, unix))]
mod tests;
