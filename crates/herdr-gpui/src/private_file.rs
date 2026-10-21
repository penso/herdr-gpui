//! Creating a database's directory and file privately: owned by this user,
//! not a link, and closed to the group and others, since issue bodies, run
//! messages, alerts, and paths go in.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not prepare {}: {source}", path.display())]
    Prepare {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a private file or directory owned by this user", path.display())]
    NotPrivate { path: PathBuf },
}

/// Creates `path`'s directory and the empty file when missing, and checks
/// that both are private.
pub(crate) fn prepare(path: &Path) -> Result<()> {
    let prepare = |path: &Path| {
        let path = path.to_owned();
        move |source| Error::Prepare { path, source }
    };
    let directory = path.parent().ok_or_else(|| Error::NotPrivate {
        path: path.to_owned(),
    })?;
    if let Some(parent) = directory.parent() {
        fs::create_dir_all(parent).map_err(prepare(parent))?;
    }
    match create_directory(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(prepare(directory)(error)),
    }
    check(directory, true)?;
    match create_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(prepare(path)(error)),
    }
    check(path, false)
}

#[cfg(unix)]
fn create_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_directory(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

#[cfg(unix)]
fn create_file(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map(drop)
}

#[cfg(not(unix))]
fn create_file(path: &Path) -> io::Result<()> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(drop)
}

/// The directory or file must be ours, not a link, and the file must have
/// one name. Neither may grant the group or others anything: issue
/// bodies, run messages, and paths go in. Permissions are not repaired;
/// agent-launcher makes both private, so a shared one passes.
fn check(path: &Path, directory: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|source| Error::Prepare {
        path: path.to_owned(),
        source,
    })?;
    let kind = if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    if !kind || metadata.file_type().is_symlink() || !owned(&metadata, directory) {
        return Err(Error::NotPrivate {
            path: path.to_owned(),
        });
    }
    Ok(())
}

#[cfg(unix)]
fn owned(metadata: &fs::Metadata, directory: bool) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.uid() == rustix::process::getuid().as_raw()
        && metadata.mode() & 0o077 == 0
        && (directory || metadata.nlink() == 1)
}

#[cfg(not(unix))]
fn owned(_: &fs::Metadata, _: bool) -> bool {
    true
}
