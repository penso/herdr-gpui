//! Noticing saves to the copies. A background run stats every copy and
//! hashes one only once two runs in a row saw the same changed metadata, so
//! an editor's write-then-rename save has settled before it counts, without
//! waiting on a clock. Blocking: runs on the background executor.

use super::{Key, push::Hashing, sync::Sha256};
use std::{
    io,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// A copy's modification time and length, which change when it is saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Meta {
    pub(crate) modified: SystemTime,
    pub(crate) len: u64,
}

impl Meta {
    pub(crate) fn of(metadata: &std::fs::Metadata) -> io::Result<Self> {
        Ok(Self {
            modified: metadata.modified()?,
            len: metadata.len(),
        })
    }
}

/// What the runs remember of one copy between them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Watch {
    /// Changed metadata the last run saw, not yet seen twice.
    pub(crate) pending: Option<Meta>,
    /// The metadata of the last save acted on.
    pub(crate) handled: Option<Meta>,
}

/// What one run makes of a copy's metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Debounce {
    /// As synced, or the save already acted on.
    Quiet,
    /// Changed, and not yet seen twice.
    Pending(Meta),
    /// The same change as the last run saw: a save to hash.
    Settled(Meta),
}

/// What `seen` means for a copy synced at `synced` and watched as `watch`.
pub(crate) fn debounce(synced: Meta, watch: &Watch, seen: Meta) -> Debounce {
    if seen == synced || watch.handled == Some(seen) {
        Debounce::Quiet
    } else if watch.pending == Some(seen) {
        Debounce::Settled(seen)
    } else {
        Debounce::Pending(seen)
    }
}

/// One copy as a run starts on it.
#[derive(Debug, Clone)]
pub(crate) struct Probe {
    pub(crate) key: Key,
    pub(crate) local: PathBuf,
    pub(crate) synced: Meta,
    pub(crate) watch: Watch,
}

/// What a run found for one copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seen {
    pub(crate) key: Key,
    /// The synced metadata the run compared against: a copy synced again
    /// meanwhile ignores the finding.
    pub(crate) synced: Meta,
    pub(crate) change: Change,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Change {
    /// Not there, as while an editor's rename is underway, or unreadable.
    Skipped,
    Quiet,
    Pending(Meta),
    Settled {
        meta: Meta,
        sha256: Sha256,
    },
}

/// Stats each probe's copy, and hashes the ones whose save settled.
pub(crate) fn scan(probes: Vec<Probe>) -> Vec<Seen> {
    probes
        .into_iter()
        .map(|probe| Seen {
            change: look(&probe),
            key: probe.key,
            synced: probe.synced,
        })
        .collect()
}

fn look(probe: &Probe) -> Change {
    let seen = std::fs::metadata(&probe.local).and_then(|metadata| Meta::of(&metadata));
    let seen = match seen {
        Ok(seen) => seen,
        Err(error) => return skipped(&probe.local, &error),
    };
    match debounce(probe.synced, &probe.watch, seen) {
        Debounce::Quiet => Change::Quiet,
        Debounce::Pending(meta) => Change::Pending(meta),
        Debounce::Settled(meta) => match hash_file(&probe.local) {
            Ok(sha256) => Change::Settled { meta, sha256 },
            Err(error) => skipped(&probe.local, &error),
        },
    }
}

/// A copy that is gone is skipped silently; any other failure is logged.
fn skipped(local: &Path, error: &io::Error) -> Change {
    if error.kind() != io::ErrorKind::NotFound {
        tracing::warn!(%error, copy = %local.display(), "Remote file copy not checked for saves");
    }
    Change::Skipped
}

fn hash_file(path: &Path) -> io::Result<Sha256> {
    let mut reader = Hashing::new(std::fs::File::open(path)?);
    io::copy(&mut reader, &mut io::sink())?;
    Ok(reader.finish().1)
}
