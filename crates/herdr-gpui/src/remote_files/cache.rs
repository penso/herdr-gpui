//! The folder one run of this client keeps its copies of remote files in:
//! `remote-files/<launch>` under the client's state directory, private to
//! the user. The run holds a lock on `<launch>/.lock` while it lives, so the
//! first use in a later run removes the folders of runs that crashed or quit
//! without removing their own, and leaves a running instance's alone.

use super::{Error, Owner, Result};
use herdr_client::RemoteShell;
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    fs::{DirBuilder, File, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

/// The lock file each launch folder holds.
const LOCK: &str = ".lock";

/// The lock file's name until it is held; the sweep never opens it.
const LOCK_PENDING: &str = ".lock.new";

/// What a copy is called when the host's name for it is not a file name.
const FALLBACK_NAME: &str = "file";

/// The characters a Windows file name cannot hold, besides control characters.
const WINDOWS_REFUSED: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Hex digits of SHA-256 naming a host's or a path's folder.
const HASH_DIGITS: usize = 16;

/// The process's one cache, created at its first use.
static SHARED: Mutex<Option<Arc<Cache>>> = Mutex::new(None);

/// One run's launch folder, locked while this value lives.
#[derive(Debug)]
pub(crate) struct Cache {
    launch: PathBuf,
    /// Released before the folder is removed, which Windows refuses while
    /// a file in it is open.
    lock: Mutex<Option<File>>,
}

impl Cache {
    /// Creates a new launch folder under `root`, holds its lock, and removes
    /// the launch folders under `root` that no running process holds.
    pub(crate) fn create(root: &Path) -> Result<Self> {
        create_private_dirs(root)?;
        let launch = root.join(launch_name());
        create_private_dirs(&launch)?;
        // The lock is taken under a name the sweep ignores and only then
        // renamed to `.lock`: another launch's sweep that found an unheld
        // `.lock` in the moment between creating and locking it would take
        // the lock and remove this live folder. The lock belongs to the open
        // file, so it survives the rename.
        let pending = launch.join(LOCK_PENDING);
        let lock = private_options()
            .create_new(true)
            .open(&pending)
            .map_err(|source| Error::Cache {
                path: pending.clone(),
                source,
            })?;
        lock.try_lock().map_err(|error| Error::Cache {
            path: pending.clone(),
            source: error.into(),
        })?;
        let lock_path = launch.join(LOCK);
        std::fs::rename(&pending, &lock_path).map_err(|source| Error::Cache {
            path: lock_path,
            source,
        })?;
        sweep(root, &launch);
        Ok(Self {
            launch,
            lock: Mutex::new(Some(lock)),
        })
    }

    /// Where `owner`'s copy of `path` on `target` is kept: a folder per
    /// window, per host, and per path, so copies never collide, holding the
    /// file under the host's own name for it, so an application shows its
    /// real name and type, split from its folders at `shell`'s separators.
    pub(crate) fn copy_path(
        &self,
        owner: Owner,
        target: &str,
        path: &str,
        shell: RemoteShell,
    ) -> PathBuf {
        self.launch
            .join(format!("w{}", owner.0))
            .join(short_hash(target))
            .join(short_hash(path))
            .join(local_name(basename(path, shell), cfg!(windows)).as_ref())
    }

    #[cfg(test)]
    pub(crate) fn launch(&self) -> &Path {
        &self.launch
    }

    /// Releases the lock and removes the launch folder with every copy in it.
    pub(crate) fn remove(&self) -> Result<()> {
        drop(
            self.lock
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take(),
        );
        match std::fs::remove_dir_all(&self.launch) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(Error::Cache {
                path: self.launch.clone(),
                source: error,
            }),
            _ => Ok(()),
        }
    }
}

/// The process's cache, created under the state directory at first use.
/// Blocking: call only on a background worker.
pub(crate) fn shared() -> Result<Arc<Cache>> {
    // A poisoned lock still holds either no cache or a whole one.
    let mut shared = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(cache) = shared.as_ref() {
        return Ok(cache.clone());
    }
    let root = crate::preferences::state_dir()
        .ok_or(Error::NoStateDir)?
        .join("remote-files");
    let cache = Arc::new(Cache::create(&root)?);
    *shared = Some(cache.clone());
    Ok(cache)
}

/// The last segment of a host's path, split at the separators of the host's
/// `shell`, or `file` when that is no plain file name. A backslash is a
/// plain name character on a POSIX host.
pub(crate) fn basename(path: &str, shell: RemoteShell) -> &str {
    let separators: &[char] = match shell {
        RemoteShell::Posix => &['/'],
        RemoteShell::PowerShell => &['/', '\\'],
    };
    let name = path.rsplit(separators).next().unwrap_or_default();
    if matches!(name, "" | "." | "..") || name.chars().any(char::is_control) {
        FALLBACK_NAME
    } else {
        name
    }
}

/// The name a copy of the host's file `name` is stored under. A Windows
/// client cannot store every name a host can, so there a character Windows
/// refuses becomes `_`, as does a trailing dot or space, and a device name
/// such as `CON` or `com1.txt` gains a leading `_`. Only the copy is
/// renamed: the host's path stays the copy's key, so uploads still go there.
pub(super) fn local_name(name: &str, windows: bool) -> Cow<'_, str> {
    if !windows {
        return Cow::Borrowed(name);
    }
    let mut local: String = name
        .chars()
        .map(|c| {
            if c.is_control() || WINDOWS_REFUSED.contains(&c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    if local.ends_with(['.', ' ']) {
        local.pop();
        local.push('_');
    }
    if windows_device(&local) {
        local.insert(0, '_');
    }
    if local == name {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(local)
    }
}

/// Whether Windows reads `name` as a device, whatever its extension.
fn windows_device(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or_default();
    let numbered = |prefix: &str| {
        stem.get(..3)
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
            && matches!(stem.as_bytes().get(3..), Some([b'1'..=b'9']))
    };
    ["CON", "PRN", "AUX", "NUL"]
        .iter()
        .any(|device| stem.eq_ignore_ascii_case(device))
        || numbered("COM")
        || numbered("LPT")
}

/// Where a fetch writes the copy at `local` before renaming it over it:
/// beside it, under a name no other fetch of this process has, so two
/// fetches of one copy never write or remove each other's bytes.
pub(super) fn part_path(local: &Path) -> PathBuf {
    beside(local, "part")
}

/// Where a fetch moves the copy at `local` aside while it puts the host's
/// bytes in its place: beside it, under a name no other fetch of this
/// process has.
pub(super) fn kept_path(local: &Path) -> PathBuf {
    beside(local, "kept")
}

/// `local`'s name with a serial number unique in this process and
/// `extension` added, in `local`'s folder.
fn beside(local: &Path, extension: &str) -> PathBuf {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let mut name = local.file_name().unwrap_or_default().to_owned();
    name.push(format!(".{serial}.{extension}"));
    local.with_file_name(name)
}

/// Creates `path` and any missing parents, private to the user.
pub(super) fn create_private_dirs(path: &Path) -> Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    // Windows has no mode bits; the folder inherits the private state directory's ACL.
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|source| Error::Cache {
        path: path.to_owned(),
        source,
    })
}

/// Creates the file at `path`, readable and writable only by the user and
/// never executable. Fails when a file is already there.
pub(super) fn create_private_file(path: &Path) -> Result<File> {
    private_options()
        .create_new(true)
        .open(path)
        .map_err(|source| Error::Cache {
            path: path.to_owned(),
            source,
        })
}

/// Options that write a file only the user may read or write, with no
/// execute bit, when they create it.
fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true);
    // Windows has no mode bits; the file inherits its folder's ACL.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

/// A folder name no other launch has: this process's ID, the time, and a
/// count of the caches it made.
fn launch_name() -> String {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    format!("{}-{nanos:x}-{serial}", std::process::id())
}

/// The first digits of `text`'s SHA-256 in hex.
fn short_hash(text: &str) -> String {
    let digest: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    digest
        .iter()
        .take(HASH_DIGITS / 2)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Removes every launch folder under `root` but `own` whose lock no process
/// holds. A folder without a lock file may be one another run is creating
/// right now, so it is left alone.
fn sweep(root: &Path, own: &Path) {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(%error, root = %root.display(), "Remote file cache not swept");
            return;
        }
    };
    for folder in entries.flatten().map(|entry| entry.path()) {
        if folder == own || !folder.is_dir() {
            continue;
        }
        let Ok(lock) = OpenOptions::new().write(true).open(folder.join(LOCK)) else {
            continue;
        };
        if lock.try_lock().is_err() {
            continue;
        }
        drop(lock);
        if let Err(error) = std::fs::remove_dir_all(&folder) {
            tracing::warn!(%error, folder = %folder.display(), "Stale remote file copies not removed");
        }
    }
}
