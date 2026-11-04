//! Opt-in Unix plaintext storage: one private file per record in a directory
//! the caller names. All callers run on a background worker.
//!
//! Each call takes the record's check, which must also bound its size. The
//! check returns the caller's own error type, so a feature's validation
//! failure reaches it unchanged while this module's failures convert into it.
#![forbid(unsafe_code)]

use crate::{Error, Result};
use secrecy::SecretString;
use std::{ffi::CStr, path::Path};

#[cfg(unix)]
use crate::LIMIT;
#[cfg(unix)]
use rustix::fs::{AtFlags, Mode, OFlags, open, openat, renameat, unlinkat};
#[cfg(unix)]
use rustix::process::geteuid;
#[cfg(unix)]
use secrecy::ExposeSecret;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
use std::{
    fs::File,
    io::{Read, Write},
};
#[cfg(unix)]
use zeroize::Zeroizing;

/// Keeping a record private on disk here depends on `openat`, POSIX
/// ownership, and mode bits. Off POSIX there is no equivalent this crate can
/// rely on, so `Store::File` is never selected and only removal, which has
/// nothing to remove, succeeds.
#[cfg(not(unix))]
pub fn read_checked<E: From<Error>>(
    _path: &Path,
    _name: &CStr,
    _validate: fn(&SecretString) -> Result<(), E>,
) -> Result<Option<SecretString>, E> {
    Ok(None)
}

#[cfg(not(unix))]
pub fn store_checked<E: From<Error>>(
    _path: &Path,
    _name: &CStr,
    token: Option<&SecretString>,
    _plaintext: bool,
    _validate: fn(&SecretString) -> Result<(), E>,
) -> Result<(), E> {
    if token.is_some() {
        return Err(Error::Unsupported.into());
    }
    Ok(())
}

#[cfg(unix)]
fn directory(path: &Path) -> Result<File> {
    let dir = File::from(
        open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| Error::Io(error.into()))?,
    );
    let metadata = dir.metadata().map_err(Error::Io)?;
    if metadata.uid() != geteuid().as_raw() || metadata.mode() & 0o022 != 0 {
        return Err(Error::Permissions);
    }
    Ok(dir)
}

#[cfg(unix)]
fn existing(dir: &File, name: &CStr) -> Result<Option<File>> {
    // Keep access relative to the validated directory, even if its path is replaced.
    let file = match openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => File::from(fd),
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(error) => return Err(Error::Io(error.into())),
    };
    let metadata = file.metadata().map_err(Error::Io)?;
    if !metadata.is_file()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o600
        || metadata.nlink() != 1
        || metadata.len() > LIMIT as u64
    {
        return Err(Error::Permissions);
    }
    Ok(Some(file))
}

/// The record `name` in `path`, once `validate` accepts it, or `None` when
/// there is none. `validate` must bound its size.
#[cfg(unix)]
pub fn read_checked<E: From<Error>>(
    path: &Path,
    name: &CStr,
    validate: fn(&SecretString) -> Result<(), E>,
) -> Result<Option<SecretString>, E> {
    let dir = directory(path)?;
    let Some(file) = existing(&dir, name)? else {
        return Ok(None);
    };
    let mut bytes = Zeroizing::new(Vec::with_capacity(LIMIT + 1));
    file.take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(Error::Io)?;
    let text = std::str::from_utf8(&bytes).map_err(Error::Encoding)?;
    let value = SecretString::from(text);
    validate(&value)?;
    Ok(Some(value))
}

/// Writes `token` as the record `name` in `path`, replacing it atomically,
/// or removes the record when `token` is `None`. Writing needs `plaintext`,
/// the caller's opt-in; removal never does. `validate` must bound its size.
#[cfg(unix)]
pub fn store_checked<E: From<Error>>(
    path: &Path,
    name: &CStr,
    token: Option<&SecretString>,
    plaintext: bool,
    validate: fn(&SecretString) -> Result<(), E>,
) -> Result<(), E> {
    if token.is_some() && !plaintext {
        return Err(Error::Policy.into());
    }
    write_checked(path, name, token, validate)
}

#[cfg(unix)]
fn write_checked<E: From<Error>>(
    path: &Path,
    target: &CStr,
    token: Option<&SecretString>,
    validate: fn(&SecretString) -> Result<(), E>,
) -> Result<(), E> {
    let dir = directory(path)?;
    let present = existing(&dir, target)?.is_some();
    let Some(token) = token else {
        if present {
            unlinkat(&dir, target, AtFlags::empty()).map_err(|error| Error::Io(error.into()))?;
        }
        return Ok(dir.sync_all().map_err(Error::Io)?);
    };
    validate(token)?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = format!(
        ".github-credentials-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let mut file = File::from(
        openat(
            &dir,
            name.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|error| Error::Io(error.into()))?,
    );
    let result = (|| {
        file.write_all(token.expose_secret().as_bytes())
            .map_err(Error::Io)?;
        file.sync_all().map_err(Error::Io)?;
        existing(&dir, target)?;
        // Replace the directory entry atomically, never a symlink's target.
        renameat(&dir, name.as_str(), &dir, target).map_err(|error| Error::Io(error.into()))?;
        dir.sync_all().map_err(Error::Io)
    })();
    let _ = unlinkat(&dir, name.as_str(), AtFlags::empty());
    Ok(result?)
}

#[cfg(all(test, unix))]
mod tests;
