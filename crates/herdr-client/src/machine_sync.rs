//! Machine sync: every machine in the user's list of saved SSH devices can
//! reach every other, from whichever one Herdr UI runs on.
//!
//! A sync reads each member over SSH (creating its `~/.ssh/herdr_ed25519`),
//! then on each one authorizes every other machine's herdr key, appends the
//! machines its catalog lacks, and, when it has none, saves a sync state of
//! its own so it keeps the list in step too. Devices another machine lists
//! are saved here. Nothing a catalog lists is ever changed or removed, and a
//! catalog that changed between the read and the write is left for next time.
//!
//! The state lives beside upstream's catalog in `machine-sync.json`: this
//! machine's profile ID and label in the others' catalogs, the target they
//! reach it by, and the saved devices left out. Without the file, sync is off.
use crate::{
    SavedHost, StorageOperation,
    catalog::{self, MAX_HOSTS, catalog_path, validate_target, write_private},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    thread,
};

mod error;
mod keys;
mod plan;
mod remote;
pub use error::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

const STATE_FILE: &str = "machine-sync.json";
const STATE_LIMIT: u64 = 16 * 1024;

/// This machine's place in the synced list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncState {
    /// This machine's profile ID in the other machines' catalogs.
    pub id: String,
    /// Its name there.
    pub label: String,
    /// The `[user@]host` the other machines reach it by.
    pub target: String,
    /// Profile IDs of saved devices left out of sync.
    pub excluded: BTreeSet<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateFile {
    version: u32,
    id: String,
    label: String,
    target: String,
    #[serde(default)]
    excluded: BTreeSet<String>,
}

impl SyncState {
    /// A state for this machine, refused unless the other machines could
    /// save it in their catalogs as it is.
    pub fn new(id: String, label: String, target: String) -> Result<Self> {
        let state = Self {
            id,
            label: label.trim().to_owned(),
            target: target.trim().to_owned(),
            excluded: BTreeSet::new(),
        };
        state.validate()?;
        Ok(state)
    }

    fn validate(&self) -> Result<()> {
        let label_ok = !self.label.trim().is_empty()
            && self.label.len() <= 128
            && !self.label.chars().any(char::is_control);
        let valid = catalog::valid_profile_id(&self.id)
            && label_ok
            && validate_target(&self.target).is_ok()
            && self.excluded.len() <= MAX_HOSTS
            && self.excluded.iter().all(|id| catalog::valid_profile_id(id));
        valid.then_some(()).ok_or(Error::StateValue)
    }

    /// Whether the saved device `id` takes part.
    pub fn includes(&self, id: &str) -> bool {
        !self.excluded.contains(id)
    }
}

pub(crate) fn parse_state(bytes: &[u8]) -> Result<SyncState> {
    let file: StateFile = serde_json::from_slice(bytes).map_err(Error::StateSchema)?;
    if file.version != 1 {
        return Err(Error::StateValue);
    }
    let state = SyncState {
        id: file.id,
        label: file.label,
        target: file.target,
        excluded: file.excluded,
    };
    state.validate()?;
    Ok(state)
}

pub(crate) fn encode_state(state: &SyncState) -> Result<String> {
    state.validate()?;
    serde_json::to_string_pretty(&StateFile {
        version: 1,
        id: state.id.clone(),
        label: state.label.clone(),
        target: state.target.clone(),
        excluded: state.excluded.clone(),
    })
    .map_err(Error::StateSchema)
}

fn state_path(development: bool) -> PathBuf {
    catalog_path(development, |name| env::var_os(name)).with_file_name(STATE_FILE)
}

/// This machine's sync state, or `None` while sync is off.
/// Blocking file I/O: call it from a background worker.
pub fn load_sync_state(development: bool) -> crate::Result<Option<SyncState>> {
    load_state_path(&state_path(development))
}

fn load_state_path(path: &Path) -> crate::Result<Option<SyncState>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(crate::Error::storage(StorageOperation::Open, path, error)),
    };
    let mut bytes = Vec::new();
    file.take(STATE_LIMIT)
        .read_to_end(&mut bytes)
        .map_err(|error| crate::Error::storage(StorageOperation::Read, path, error))?;
    parse_state(&bytes)
        .map(Some)
        .map_err(|error| crate::Error::storage(StorageOperation::Decode, path, error))
}

/// Saves this machine's sync state, turning sync on, or with `None` removes
/// it, turning sync off here. Other machines keep what earlier syncs gave
/// them. Blocking file I/O: call it from a background worker.
pub fn store_sync_state(development: bool, state: Option<&SyncState>) -> crate::Result<()> {
    store_state_path(&state_path(development), state)
}

fn store_state_path(path: &Path, state: Option<&SyncState>) -> crate::Result<()> {
    let Some(state) = state else {
        return match fs::remove_file(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                Err(crate::Error::storage(StorageOperation::Write, path, error))
            }
            _ => Ok(()),
        };
    };
    let content = encode_state(state)
        .map_err(|error| crate::Error::storage(StorageOperation::Encode, path, error))?;
    write_private(path, content.as_bytes())
}

/// What one sync did on a member.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MachineChange {
    /// Keys of other machines it now accepts.
    pub keys: usize,
    /// Saved devices it gained.
    pub devices: usize,
}

/// One member's part of a sync.
#[derive(Debug)]
pub struct MachineReport {
    pub id: String,
    pub label: String,
    pub result: Result<MachineChange>,
}

#[derive(Debug, Default)]
pub struct SyncReport {
    /// Every saved device that takes part, in catalog order.
    pub machines: Vec<MachineReport>,
    /// Devices another machine listed, now saved here too.
    pub learned: Vec<SavedHost>,
    /// Keys of other machines this one now accepts.
    pub authorized: usize,
}

/// Syncs the list as `state` says. Members that cannot be reached are
/// reported and left for the next sync; an error means nothing was written
/// on any other machine. Blocks on SSH for every member at once: call it from
/// a background worker. Linux and macOS clients only, as host scripts are.
pub fn sync_machines(
    development: bool,
    state: &SyncState,
    cancelled: &AtomicBool,
) -> crate::Result<SyncReport> {
    state.validate()?;
    let own_key = keys::ensure_local_key()?;
    let catalog_file = catalog_path(development, |name| env::var_os(name));
    let local = catalog::load_catalog(&catalog_file)?.ssh;
    let hosts: Vec<&SavedHost> = local
        .iter()
        .filter(|host| host.enabled && state.includes(&host.id))
        .collect();
    let collected: Vec<Result<remote::Collected>> = thread::scope(|scope| {
        let workers: Vec<_> = hosts
            .iter()
            .map(|host| scope.spawn(move || remote::collect(&host.target, cancelled)))
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap_or(Err(Error::Reply)))
            .collect()
    });
    let members: Vec<plan::Member> = hosts
        .iter()
        .zip(&collected)
        .map(|(host, collected)| plan::Member {
            host,
            collected: collected.as_ref().ok(),
        })
        .collect();
    let plan = plan::plan(state, &own_key, &local, &members)?;
    // This machine first: should it fail, no other machine was changed.
    let authorized = keys::authorize_locally(&plan.local_keys)?;
    catalog::add_hosts(&catalog_file, &plan.learned)?;
    let results: Vec<Result<MachineChange>> = thread::scope(|scope| {
        let workers: Vec<_> = members
            .iter()
            .zip(&plan.writes)
            .map(|(member, write)| {
                scope.spawn(move || {
                    let (Some(collected), Some(write)) = (member.collected, write) else {
                        return Err(Error::Reply);
                    };
                    let keys = remote::apply(
                        &member.host.target,
                        &remote::Write {
                            sum: &collected.sum,
                            keys: &write.keys,
                            catalog: write.catalog.as_deref(),
                            state: write.state.as_deref(),
                        },
                        cancelled,
                    )?;
                    Ok(MachineChange {
                        keys,
                        devices: write.devices,
                    })
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap_or(Err(Error::Reply)))
            .collect()
    });
    let machines = hosts
        .iter()
        .zip(collected.into_iter().zip(results))
        .map(|(host, (collected, result))| MachineReport {
            id: host.id.clone(),
            label: host.label.clone(),
            result: collected.and(result),
        })
        .collect();
    Ok(SyncReport {
        machines,
        learned: plan.learned,
        authorized,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
