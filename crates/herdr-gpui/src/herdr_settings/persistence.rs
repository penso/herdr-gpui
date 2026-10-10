//! Descriptor-relative Unix persistence. The lock coordinates this adapter's
//! writers, not arbitrary editors: the last comparison is optimistic, not a
//! filesystem compare-and-swap. No portable rename API can eliminate that final
//! race with a non-cooperating writer. Symlinks are expanded only by `resolve`;
//! every later open refuses to follow anything. A link retargeted during a save
//! leaves the same files as a save that finished just before it; one retargeted
//! between load and save resolves to a different file, which is a conflict.
use super::Error;
use rustix::fs::{
    AtFlags, FlockOperation, Mode, OFlags, flock, linkat, mkdirat, open, openat, renameat, statat,
    unlinkat,
};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, Metadata},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) const LIMIT: u64 = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    device: u64,
    inode: u64,
    mode: u32,
    uid: u32,
    gid: u32,
}

impl Identity {
    fn of(meta: &Metadata) -> Self {
        Self {
            device: meta.dev(),
            inode: meta.ino(),
            mode: meta.mode(),
            uid: meta.uid(),
            gid: meta.gid(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub text: Option<String>,
    pub identity: Option<Identity>,
    pub directory: Option<Identity>,
    pub revision: Option<[i64; 4]>,
}

fn revision(meta: &Metadata) -> [i64; 4] {
    [
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
    ]
}

fn io(error: rustix::io::Errno) -> Error {
    if error == rustix::io::Errno::LOOP {
        Error::UnsafePath
    } else {
        Error::Io(error.into())
    }
}

fn parts(path: &Path) -> Result<(&Path, &OsStr), Error> {
    Ok((
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or(Error::UnsafePath)?,
        path.file_name().ok_or(Error::UnsafePath)?,
    ))
}

/// Only this user or root may own a directory on the config path, and no one
/// else may write to it unless root owns it with the sticky bit set (`/tmp`).
fn validate_directory(meta: &Metadata, uid: u32, path: &Path) -> Result<(), Error> {
    if meta.uid() != uid && meta.uid() != 0 {
        return Err(Error::UnsafePath);
    }
    if meta.uid() == 0 && meta.mode() & 0o1000 != 0 {
        return Ok(());
    }
    validate_permissions(meta, path)
}

fn validate_permissions(meta: &Metadata, path: &Path) -> Result<(), Error> {
    if meta.mode() & 0o022 != 0 {
        return Err(Error::InsecurePermissions {
            path: path.to_owned(),
            mode: meta.mode() & 0o7777,
        });
    }
    Ok(())
}

fn directory(path: &Path, create: bool) -> Result<File, Error> {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let uid = rustix::process::geteuid().as_raw();
    let mut dir = File::from(open("/", flags, Mode::empty()).map_err(io)?);
    let mut current_path = PathBuf::from("/");
    for component in path.components() {
        let name = match component {
            Component::RootDir | Component::CurDir => continue,
            Component::Normal(name) => name,
            _ => return Err(Error::UnsafePath),
        };
        validate_directory(&dir.metadata()?, uid, &current_path)?;
        match statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW) {
            Err(rustix::io::Errno::NOENT) if create => {
                match mkdirat(&dir, name, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
                    Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                    Err(error) => return Err(io(error)),
                }
            }
            Ok(_) => {}
            Err(error) => return Err(io(error)),
        }
        dir = File::from(openat(&dir, name, flags, Mode::empty()).map_err(
            |error| match error {
                rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR => Error::UnsafePath,
                _ => io(error),
            },
        )?);
        current_path.push(name);
    }
    let meta = dir.metadata()?;
    if meta.uid() != uid {
        return Err(Error::UnsafePath);
    }
    validate_permissions(&meta, path)?;
    Ok(dir)
}

/// Herdr reads config.toml through symlinks, so dotfile managers commonly link
/// it, or a directory above it, elsewhere. The path as given is checked first:
/// every existing entry must sit in a trusted directory, and any link on it must
/// belong to this user or root, so another user cannot plant or swap a link even
/// in a sticky directory. Links are then expanded once, as the kernel would,
/// keeping a not-yet-created tail as given. `directory` and `read_at` revalidate
/// the physical path without following anything, so a link swapped in afterwards
/// is refused, and saves replace the link's target in its own directory while
/// leaving the link in place. Links nested inside a link's target are not
/// checked here; only the physical path they lead to is.
fn resolve(path: &Path) -> Result<PathBuf, Error> {
    // A relative path would leave the working directory's ancestors unchecked.
    if !path.is_absolute() {
        return Err(Error::UnsafePath);
    }
    let uid = rustix::process::geteuid().as_raw();
    let mut real = None;
    for existing in path.ancestors() {
        let entry = match fs::symlink_metadata(existing) {
            Ok(entry) => entry,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if entry.file_type().is_symlink() && entry.uid() != uid && entry.uid() != 0 {
            return Err(Error::UnsafePath);
        }
        if let Some(parent) = existing.parent().filter(|p| !p.as_os_str().is_empty()) {
            validate_directory(&fs::metadata(parent)?, uid, parent)?;
        }
        if real.is_none() {
            match fs::canonicalize(existing) {
                Ok(target) => {
                    let missing = path.strip_prefix(existing).map_err(|_| Error::UnsafePath)?;
                    real = Some(target.join(missing));
                }
                // A dangling link stays as given, and `read_at` refuses it.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    real.ok_or(Error::UnsafePath)
}

fn validate_file(meta: &Metadata, path: &Path) -> Result<(), Error> {
    if !meta.is_file()
        || meta.uid() != rustix::process::geteuid().as_raw()
        || meta.mode() & 0o7000 != 0
        || meta.nlink() != 1
    {
        return Err(Error::UnsafePath);
    }
    validate_permissions(meta, path)
}

fn read_at(dir: &File, name: &OsStr, path: &Path) -> Result<Snapshot, Error> {
    let mut snapshot = Snapshot {
        directory: Some(Identity::of(&dir.metadata()?)),
        ..Snapshot::default()
    };
    let file = match openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(file) => File::from(file),
        Err(rustix::io::Errno::NOENT) => return Ok(snapshot),
        Err(rustix::io::Errno::LOOP) => return Err(Error::UnsafePath),
        Err(error) => return Err(io(error)),
    };
    let before = file.metadata()?;
    validate_file(&before, path)?;
    if before.len() > LIMIT {
        return Err(Error::TooLarge);
    }
    let mut text = String::new();
    (&file).take(LIMIT + 1).read_to_string(&mut text)?;
    if text.len() as u64 > LIMIT {
        return Err(Error::TooLarge);
    }
    let after = file.metadata()?;
    if Identity::of(&before) != Identity::of(&after)
        || before.len() != after.len()
        || revision(&before) != revision(&after)
        || before.nlink() != after.nlink()
    {
        return Err(Error::Conflict);
    }
    snapshot.identity = Some(Identity::of(&after));
    snapshot.revision = Some(revision(&after));
    snapshot.text = Some(text);
    Ok(snapshot)
}

pub(super) fn read(path: &Path) -> Result<Snapshot, Error> {
    let path = resolve(path)?;
    let (parent, name) = parts(&path)?;
    let dir = match directory(parent, false) {
        Ok(dir) => dir,
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Snapshot::default());
        }
        Err(error) => return Err(error),
    };
    read_at(&dir, name, &path)
}

fn unchanged(original: &Snapshot, current: &Snapshot) -> Result<(), Error> {
    if original.text != current.text
        || original.identity != current.identity
        || original.revision != current.revision
        || original
            .directory
            .as_ref()
            .is_some_and(|dir| Some(dir) != current.directory.as_ref())
    {
        return Err(Error::Conflict);
    }
    Ok(())
}

struct LockGuard(File);

impl Drop for LockGuard {
    fn drop(&mut self) {
        // A forked child can retain a duplicate until exec despite CLOEXEC.
        // Explicit unlock releases the lock without waiting for its last close.
        let _ = flock(&self.0, FlockOperation::Unlock);
    }
}

pub(super) fn save(path: &Path, original: &Snapshot, text: &str) -> Result<Snapshot, Error> {
    if text.len() as u64 > LIMIT {
        return Err(Error::TooLarge);
    }
    let path = resolve(path)?;
    let (parent, name) = parts(&path)?;
    let dir = directory(parent, true)?;
    let mut lock_name = OsString::from(".");
    lock_name.push(name);
    lock_name.push(".gpui-lock");
    // Keep this lock inode permanently. Unlinking it could let two writers lock
    // different inodes. The guard explicitly unlocks on every return.
    let lock = File::from(
        openat(
            &dir,
            lock_name.as_os_str(),
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(io)?,
    );
    validate_file(&lock.metadata()?, &parent.join(&lock_name))?;
    match flock(&lock, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => {}
        Err(rustix::io::Errno::WOULDBLOCK) => return Err(Error::Busy),
        Err(error) => return Err(io(error)),
    }
    let _lock = LockGuard(lock);
    let current = read_at(&dir, name, &path)?;
    unchanged(original, &current)?;

    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut temporary = None;
    for _ in 0..128 {
        let name = format!(
            ".herdr-gpui-settings-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        match openat(
            &dir,
            name.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        ) {
            Ok(fd) => {
                temporary = Some((name, File::from(fd)));
                break;
            }
            Err(rustix::io::Errno::EXIST) => {}
            Err(error) => return Err(io(error)),
        }
    }
    let (temp_name, mut file) = temporary.ok_or(Error::Busy)?;
    let result = (|| {
        file.write_all(text.as_bytes())?;
        if let Some(identity) = &current.identity {
            // Preserve owner/group and mode, rather than making a shared config
            // more permissive or silently changing its group during replacement.
            rustix::fs::fchown(
                &file,
                None,
                Some(rustix::process::Gid::from_raw(identity.gid)),
            )
            .map_err(io)?;
            file.set_permissions(fs::Permissions::from_mode(identity.mode & 0o777))?;
        }
        file.sync_all()?;
        unchanged(&current, &read_at(&dir, name, &path)?)?;
        if Identity::of(&directory(parent, false)?.metadata()?) != Identity::of(&dir.metadata()?) {
            return Err(Error::Conflict);
        }
        let identity = Identity::of(&file.metadata()?);
        if current.text.is_none() {
            // Unlike rename, this cannot overwrite a newly created config.
            match linkat(&dir, temp_name.as_str(), &dir, name, AtFlags::empty()) {
                Ok(()) => {}
                Err(rustix::io::Errno::EXIST) => return Err(Error::Conflict),
                Err(error) => return Err(io(error)),
            }
            unlinkat(&dir, temp_name.as_str(), AtFlags::empty())
                .map_err(|error| Error::Committed(error.into()))?;
        } else {
            renameat(&dir, temp_name.as_str(), &dir, name).map_err(io)?;
        }
        // Failure here means replacement happened but durability is uncertain.
        // The caller must reload, not retry against the original snapshot.
        dir.sync_all().map_err(Error::Committed)?;
        Ok(Snapshot {
            text: Some(text.into()),
            identity: Some(identity),
            directory: current.directory,
            revision: Some(revision(&file.metadata().map_err(Error::Committed)?)),
        })
    })();
    // Cleanup is descriptor-relative too, so replacing the parent path cannot
    // redirect removal into a different directory. Missing means committed.
    if let Err(cleanup) = unlinkat(&dir, temp_name.as_str(), AtFlags::empty())
        && cleanup != rustix::io::Errno::NOENT
    {
        return Err(Error::Cleanup {
            source: result.err().map(Box::new),
            cleanup: cleanup.into(),
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_unlocks_while_duplicate_descriptor_remains_open() -> anyhow::Result<()> {
        let temp = tempfile::NamedTempFile::new()?;
        let lock = temp.reopen()?;
        flock(&lock, FlockOperation::NonBlockingLockExclusive)?;
        let duplicate = lock.try_clone()?;
        let guard = LockGuard(lock);
        let contender = temp.reopen()?;
        assert_eq!(
            flock(&contender, FlockOperation::NonBlockingLockExclusive),
            Err(rustix::io::Errno::WOULDBLOCK)
        );
        drop(guard);
        flock(&contender, FlockOperation::NonBlockingLockExclusive)?;
        let _contender = LockGuard(contender);
        drop(duplicate);
        Ok(())
    }
}
