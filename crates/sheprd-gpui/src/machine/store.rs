//! A machine's folder on disk. Everything here reaches the disk, so it runs
//! off the main thread.

use super::{Error, Name, Result, Settings};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::ErrorKind,
    os::unix::fs::MetadataExt as _,
    path::{Path, PathBuf},
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

/// What machine `name` boots with next, or the defaults.
pub(crate) fn settings(name: &Name) -> Result<Settings> {
    Ok(read_json(&folder(name)?.join(SETTINGS)).unwrap_or_default())
}

/// Keeps `settings` for machine `name`'s next boot.
pub(crate) fn save_settings(name: &Name, settings: &Settings) -> Result<()> {
    let folder = folder(name)?;
    if !folder.join(DISK).is_file() {
        return Err(Error::NotInstalled(folder));
    }
    write_json(&folder.join(SETTINGS), settings)
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

/// Replaces `path` whole, so a crash leaves the old file or the new one.
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let write_error = |source| Error::Write {
        path: path.to_owned(),
        source,
    };
    let text = serde_json::to_vec_pretty(value).map_err(|source| write_error(source.into()))?;
    let partial = path.with_extension("tmp");
    fs::write(&partial, text).map_err(write_error)?;
    fs::rename(&partial, path).map_err(write_error)
}

pub(super) fn read_error(path: &Path, source: std::io::Error) -> Error {
    Error::Read {
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests;
