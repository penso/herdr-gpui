//! A machine's folder on disk. Everything here reaches the disk, so it runs
//! off the main thread.

use super::{Error, Name, Result, Settings};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::HashMap,
    fs,
    io::{ErrorKind, Write as _},
    os::unix::fs::MetadataExt as _,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex, MutexGuard, PoisonError},
};

pub(super) const AUXILIARY: &str = "auxiliary.bin";
pub(super) const DISK: &str = "disk.img";
pub(super) const HARDWARE: &str = "hardware.bin";
pub(super) const IDENTITY: &str = "identity.bin";
/// What the user chose, for the next boot.
const SETTINGS: &str = "machine.json";
/// What the machine last booted with, which a saved state needs again.
const BOOTED: &str = "booted.json";
/// The saved state. It exists only while the machine is suspended: a state
/// resumed once no longer matches the disk.
const STATE: &str = "state.vzsave";

/// Held across a change to a machine's settings, so that a read and the write
/// it leads to are one step: the page and the command line both change them.
static SETTINGS_LOCK: Mutex<()> = Mutex::new(());

/// `stat` counts blocks of this size.
const BLOCK_BYTES: u64 = 512;

/// A machine as its folder describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Listing {
    pub(crate) name: Name,
    pub(crate) settings: Settings,
    /// Whether it is suspended, and resumes where it was.
    pub(crate) saved: bool,
    /// What it really takes on disk: the disk image is sparse.
    pub(crate) disk_bytes: u64,
}

/// The folder that holds every machine.
fn root() -> Result<PathBuf> {
    let state = crate::preferences::state_dir().ok_or(Error::NoStateDir)?;
    Ok(state.join("machines"))
}

pub(super) fn folder(name: &Name) -> Result<PathBuf> {
    Ok(root()?.join(name.as_str()))
}

pub(super) fn state_file(name: &Name) -> Result<PathBuf> {
    Ok(folder(name)?.join(STATE))
}

/// Every installed machine, by name.
pub(crate) fn list() -> Result<Vec<Listing>> {
    list_in(&root()?)
}

fn list_in(root: &Path) -> Result<Vec<Listing>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(read_error(root, source)),
    };
    let mut machines: Vec<Listing> = entries
        .flatten()
        .filter_map(|entry| {
            let name = Name::try_from(entry.file_name().into_string().ok()?).ok()?;
            let folder = entry.path();
            folder.join(DISK).is_file().then(|| listing(name, &folder))
        })
        .collect();
    machines.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
    Ok(machines)
}

fn listing(name: Name, folder: &Path) -> Listing {
    Listing {
        name,
        settings: read_json(&folder.join(SETTINGS)).unwrap_or_default(),
        saved: folder.join(STATE).is_file(),
        disk_bytes: disk_bytes(folder),
    }
}

/// The bytes the folder's files take. Each is listed by `read_dir`, which
/// does not follow links, and a machine's folder holds no subfolders.
fn disk_bytes(folder: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(folder) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.blocks().saturating_mul(BLOCK_BYTES))
        .sum()
}

/// Keeps `settings` for machine `name`'s next boot.
fn save_settings(name: &Name, settings: &Settings) -> Result<()> {
    update_settings(name, |_| Ok(*settings)).map(|_| ())
}

/// Settings on their way to disk, latest per machine. One writer drains them,
/// so an older click cannot land after a newer one. There is one queue for the
/// app: a window may close with choices in it, and a new window then adds to
/// the same queue rather than racing it.
#[derive(Debug, Default)]
struct SettingsQueue(Mutex<Queued>);

static QUEUE: LazyLock<SettingsQueue> = LazyLock::new(SettingsQueue::default);

/// Queues `settings` for machine `name`'s next boot. True when no writer
/// runs, so the caller starts [`drain_settings`].
pub(crate) fn queue_settings(name: Name, settings: Settings) -> bool {
    QUEUE.push(name, settings)
}

/// Writes what is queued, and what arrives meanwhile, off the main thread.
pub(crate) fn drain_settings() -> Result<()> {
    QUEUE.drain(save_settings)
}

#[derive(Debug, Default)]
struct Queued {
    pending: HashMap<Name, Settings>,
    draining: bool,
}

impl SettingsQueue {
    fn queued(&self) -> MutexGuard<'_, Queued> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queues `settings`. True when no writer runs, so the caller starts one.
    fn push(&self, name: Name, settings: Settings) -> bool {
        let mut queued = self.queued();
        queued.pending.insert(name, settings);
        !std::mem::replace(&mut queued.draining, true)
    }

    /// Writes all that is queued, and what arrives meanwhile, off the main
    /// thread. It returns the last failure, after trying the rest.
    fn drain(&self, mut write: impl FnMut(&Name, &Settings) -> Result<()>) -> Result<()> {
        let mut outcome = Ok(());
        loop {
            let next = {
                let mut queued = self.queued();
                let name = queued.pending.keys().next().cloned();
                let next = name.and_then(|name| {
                    let settings = queued.pending.remove(&name)?;
                    Some((name, settings))
                });
                queued.draining = next.is_some();
                next
            };
            let Some((name, settings)) = next else {
                return outcome;
            };
            if let Err(error) = write(&name, &settings) {
                outcome = Err(error);
            }
        }
    }
}

/// Keeps what `change` makes of machine `name`'s settings for its next boot,
/// and returns it.
pub(crate) fn update_settings(
    name: &Name,
    change: impl FnOnce(Settings) -> Result<Settings>,
) -> Result<Settings> {
    update_in(&folder(name)?, change)
}

fn update_in(folder: &Path, change: impl FnOnce(Settings) -> Result<Settings>) -> Result<Settings> {
    let _held = SETTINGS_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    if !folder.join(DISK).is_file() {
        return Err(Error::NotInstalled(folder.to_owned()));
    }
    let path = folder.join(SETTINGS);
    let settings = change(read_json(&path).unwrap_or_default())?;
    write_json(&path, &settings)?;
    Ok(settings)
}

/// What machine `name` must boot with now: a saved state resumes only as it
/// was saved, so it keeps the settings it booted with until it next shuts
/// down. Otherwise the user's choice, recorded for a later save.
pub(super) fn boot_settings(folder: &Path) -> Result<Settings> {
    let booted = folder.join(BOOTED);
    if folder.join(STATE).is_file()
        && let Some(settings) = read_json(&booted)
    {
        return Ok(settings);
    }
    let settings = read_json(&folder.join(SETTINGS)).unwrap_or_default();
    write_json(&booted, &settings)?;
    Ok(settings)
}

/// Forgets machine `name`'s saved state, so it next boots afresh.
pub(crate) fn discard_state(name: &Name) -> Result<()> {
    let state = state_file(name)?;
    match fs::remove_file(&state) {
        Err(source) if source.kind() != ErrorKind::NotFound => Err(Error::Delete {
            path: state,
            source,
        }),
        _ => Ok(()),
    }
}

/// Deletes machine `name` and everything it holds.
pub(crate) fn delete(name: &Name) -> Result<()> {
    delete_folder(&folder(name)?)
}

fn delete_folder(folder: &Path) -> Result<()> {
    match fs::remove_dir_all(folder) {
        Err(source) if source.kind() != ErrorKind::NotFound => Err(Error::Delete {
            path: folder.to_owned(),
            source,
        }),
        _ => Ok(()),
    }
}

/// `path` parsed, or `None` for a file missing or damaged: a machine with
/// damaged settings still boots with the defaults.
fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Replaces `path` whole, so a crash leaves the old file or the new one. The
/// temporary file is this write's own: writers cannot meet in it.
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let write_error = |source| Error::Write {
        path: path.to_owned(),
        source,
    };
    let text = serde_json::to_vec_pretty(value).map_err(|source| write_error(source.into()))?;
    let folder = path.parent().unwrap_or_else(|| Path::new("."));
    let mut partial = tempfile::NamedTempFile::new_in(folder).map_err(write_error)?;
    partial.write_all(&text).map_err(write_error)?;
    partial
        .persist(path)
        .map(|_| ())
        .map_err(|error| write_error(error.error))
}

pub(super) fn read_error(path: &Path, source: std::io::Error) -> Error {
    Error::Read {
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests;
