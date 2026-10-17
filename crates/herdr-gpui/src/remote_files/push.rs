//! Sending a copy back to its host. The push sends exactly the length the
//! copy had when it was stat'ed, and the host replaces nothing unless that
//! many bytes arrived, so a transfer cut off early never lands as a
//! truncated file. It hashes exactly the bytes it hands over, so the hash
//! stored as synced is the hash of what the host received. Blocking.

use super::{Error, Io, Result, sync::Sha256, watch::Meta};
use crate::window::RemotePath;
use herdr_client::{Expect, RemoteFileAccess};
use sha2::{Digest, Sha256 as Hasher};
use std::{
    io::{self, Read},
    path::Path,
};

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
/// `expect`, refusing a copy larger than `limit` bytes. A copy that grows
/// while it is read sends only the length it had when stat'ed, and the
/// watch sees the rest as a later save; one that shrinks fails the push. A
/// new file on the host takes the access `access` names. Blocking.
pub(crate) fn push_copy(
    io: &Io,
    remote: &RemotePath,
    local: &Path,
    (expect, access): (Expect, RemoteFileAccess<'_>),
    limit: u64,
) -> Result<Pushed> {
    let cache = |source| Error::Cache {
        path: local.to_owned(),
        source,
    };
    let file = std::fs::File::open(local).map_err(cache)?;
    let meta = file
        .metadata()
        .and_then(|metadata| Meta::of(&metadata))
        .map_err(cache)?;
    if meta.len > limit {
        return Err(Error::TooLarge { limit });
    }
    let mut reader = Hashing::new(file.take(meta.len));
    let pushed = (io.push)(remote, expect, access, meta.len, &mut reader);
    let (sent, sha256) = reader.finish();
    pushed.map_err(|error| match error {
        herdr_client::Error::RemoteFile(herdr_client::RemoteFileError::Access) => Error::Access,
        error => Error::Push(error),
    })?;
    // The host refuses a short transfer; this holds a push that ignored it.
    if sent != meta.len {
        return Err(Error::Push(herdr_client::Error::RemoteFile(
            herdr_client::RemoteFileError::Incomplete,
        )));
    }
    Ok(Pushed { meta, sha256 })
}
