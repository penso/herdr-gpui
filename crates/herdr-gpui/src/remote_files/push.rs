//! Sending a copy back to its host. The push first stages the copy in a
//! private file beside it and sends that, so an editor rewriting the copy
//! during a slow upload never makes the host receive the start of one save
//! and the end of another; a copy that changes while it is staged is staged
//! again. The push sends exactly the staged length, and the host replaces
//! nothing unless that many bytes arrived, so a transfer cut off early
//! never lands as a truncated file. It hashes exactly the bytes it hands
//! over, so the hash stored as synced is the hash of what the host
//! received. Blocking.

use super::{Error, Io, Result, cache, cache_error, sync::Sha256, watch::Meta};
use crate::window::RemotePath;
use herdr_client::{Expect, RemoteFileAccess};
use sha2::{Digest, Sha256 as Hasher};
use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

/// How many times a copy that keeps changing while it is staged is staged
/// before the push gives up; the watch then sees the latest save.
const STAGE_ATTEMPTS: usize = 3;

/// A reader that hashes and counts what it reads.
pub(crate) struct Hashing<R> {
    inner: R,
    hasher: Hasher,
    count: u64,
}

impl<R: Read> Hashing<R> {
    pub(crate) fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: Hasher::new(),
            count: 0,
        }
    }

    /// The count and SHA-256 of the bytes read.
    pub(crate) fn finish(self) -> (u64, Sha256) {
        (self.count, self.hasher.finalize().into())
    }
}

impl<R: Read> Read for Hashing<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count = self.count.saturating_add(n as u64);
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

/// A finished push: what the host now holds, and the copy's metadata from
/// before it was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pushed {
    pub(crate) meta: Meta,
    pub(crate) sha256: Sha256,
}

/// Pushes the copy at `local` to `remote` while the host's file meets
/// `expect`, refusing a copy larger than `limit` bytes. What is sent is a
/// staged snapshot of the copy, whose metadata is returned as the copy's,
/// so a save made after it is a later save to the watch. A new file on the
/// host takes the access `access` names. Blocking.
pub(crate) fn push_copy(
    io: &Io,
    remote: &RemotePath,
    local: &Path,
    (expect, access): (Expect, RemoteFileAccess<'_>),
    limit: u64,
) -> Result<Pushed> {
    let staged = Staged::of(local, limit)?;
    let file = File::open(&staged.path).map_err(|source| cache_error(&staged.path, source))?;
    let len = staged.meta.len;
    let mut reader = Hashing::new(file.take(len));
    let pushed = (io.push)(remote, expect, access, len, &mut reader);
    let (sent, sha256) = reader.finish();
    pushed.map_err(|error| match error {
        herdr_client::Error::RemoteFile(herdr_client::RemoteFileError::Access) => Error::Access,
        herdr_client::Error::RemoteFile(herdr_client::RemoteFileError::SharedFolder) => {
            Error::SharedFolder
        }
        error => Error::Push(error),
    })?;
    // The host refuses a short transfer; this holds a push that ignored it.
    if sent != len {
        return Err(Error::Push(herdr_client::Error::RemoteFile(
            herdr_client::RemoteFileError::Incomplete,
        )));
    }
    Ok(Pushed {
        meta: staged.meta,
        sha256,
    })
}

/// A private snapshot of a copy, removed when dropped.
struct Staged {
    path: PathBuf,
    /// The copy's metadata both before and after it was read.
    meta: Meta,
}

impl Staged {
    /// Copies the copy at `local` to a new private file beside it, again
    /// while its metadata changed during the read, so the snapshot holds
    /// one save whole. Refuses a copy larger than `limit` bytes.
    fn of(local: &Path, limit: u64) -> Result<Self> {
        for _ in 0..STAGE_ATTEMPTS {
            let before = meta_of(local)?;
            if before.len > limit {
                return Err(Error::TooLarge { limit });
            }
            let path = cache::staged_path(local);
            // Armed only once the file is ours, so the drop removes nothing else.
            let mut file = cache::create_private_file(&path)?;
            let staged = Self { path, meta: before };
            let source = File::open(local).map_err(|source| cache_error(local, source))?;
            let copied = io::copy(&mut source.take(before.len), &mut file)
                .map_err(|source| cache_error(&staged.path, source))?;
            drop(file);
            if copied == before.len && meta_of(local)? == before {
                return Ok(staged);
            }
        }
        Err(Error::Unsettled)
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path)
            && error.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!(%error, path = %self.path.display(), "Staged upload not removed");
        }
    }
}

fn meta_of(path: &Path) -> Result<Meta> {
    super::meta_of(path).map_err(|source| cache_error(path, source))
}
