//! The disk cache on Windows: the same slots, entries, and single writer as on
//! Unix, with std file locking and reparse-point checks in place of `flock`
//! and `O_NOFOLLOW`.
//!
//! There is no ownership or mode check. On Unix the root can come from
//! `XDG_CACHE_HOME` and so sit in a directory other users share; here it is
//! always under `%LOCALAPPDATA%`, inside the user's profile, whose inherited
//! ACL lets only that user, SYSTEM, and Administrators create, replace, or
//! link entries. Anyone else who could write there could already replace this
//! program. For the same reason hard links, which stable std cannot count
//! here, are not refused: only someone who can write the directory can make
//! one. Reparse points (symbolic links and junctions) are still refused, so an
//! entry never resolves outside the directory.

use super::{Arc, File, Image, Path, PathBuf, SystemTime, decode, entry, key, record};
use std::{
    fs::{self, OpenOptions},
    os::windows::fs::OpenOptionsExt,
};
use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

pub(in crate::avatars) fn root() -> Option<PathBuf> {
    root_in(std::env::var_os("LOCALAPPDATA"))
}

/// Unlike the Unix root, `XDG_CACHE_HOME` is not honoured: the reasoning
/// above relies on the cache living in the user's profile.
pub(super) fn root_in(local_app_data: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let base = PathBuf::from(local_app_data.filter(|p| !p.is_empty())?);
    base.is_absolute()
        .then(|| base.join(r"herdr\gpui\cache\avatars-v1"))
}

/// Holds the cache's lock file, locked exclusively, while it is in use.
pub(super) struct Directory {
    pub(super) lock: File,
}

impl Drop for Directory {
    fn drop(&mut self) {
        // A duplicated handle shares the lock; release it rather than waiting
        // for the last handle to close.
        let _ = self.lock.unlock();
    }
}

impl Directory {
    pub(super) fn open(path: &Path) -> Option<Self> {
        fs::create_dir_all(path).ok()?;
        // `is_dir` is false for a symbolic link or junction to a directory.
        if !fs::symlink_metadata(path).ok()?.is_dir() {
            return None;
        }
        let lock = open_entry(
            &path.join("lock"),
            OpenOptions::new().read(true).write(true).create(true),
        )?;
        // Never wait for another process; a contended cache is just a miss.
        lock.try_lock().ok()?;
        Some(Self { lock })
    }
}

/// Opens a regular file in the cache directory. A reparse point is opened
/// itself rather than followed, and then refused.
fn open_entry(path: &Path, options: &mut OpenOptions) -> Option<File> {
    let file = options
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .ok()?;
    file.metadata().ok()?.is_file().then_some(file)
}

pub(in crate::avatars) fn read(
    path: &Path,
    url: &str,
    now: SystemTime,
) -> Option<(Arc<Image>, bool)> {
    let _directory = Directory::open(path)?;
    let (hash, name) = key(url);
    let slot = path.join(name);
    let file = open_entry(&slot, OpenOptions::new().read(true))?;
    match entry(file, &hash, now) {
        Some((true, image, fresh)) => Some((image, fresh)),
        Some((false, _, _)) => None,
        None => {
            // `entry` closed the file, and the slot was a regular file.
            let _ = fs::remove_file(&slot);
            None
        }
    }
}

pub(in crate::avatars) fn write(
    path: &Path,
    url: &str,
    bytes: &[u8],
    now: SystemTime,
) -> Option<()> {
    decode(bytes)?;
    let _directory = Directory::open(path)?;
    let (hash, name) = key(url);
    // One temporary entry, recovered under the lock after an interrupted write.
    let pending = path.join("pending");
    let _ = fs::remove_file(&pending);
    let mut file = open_entry(&pending, OpenOptions::new().write(true).create_new(true))?;
    let written = record(&mut file, &hash, now, bytes);
    // Close before renaming, so nothing holds the entry open under its name.
    drop(file);
    // Replaces the slot itself, even when it is a reparse point.
    let result = written.and_then(|()| fs::rename(&pending, path.join(name)).ok());
    let _ = fs::remove_file(&pending);
    result
}
