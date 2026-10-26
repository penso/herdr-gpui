//! Paired devices, kept in `devices.json` in the daemon's private state
//! directory. Every change replaces the file atomically, private to this
//! user; seeing a device is written at most once a minute.

use super::{Error, NewDevice, PublicKey, Result, StoreOperation};
use crate::control::{self, DeviceId};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const FILE_NAME: &str = "devices.json";
const VERSION: u32 = 1;
/// How stale the stored last-seen time may grow before a touch writes it.
pub(crate) const TOUCH_PERSIST_MS: u64 = 60_000;

/// One paired device as the store keeps it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct DeviceRecord {
    /// The origin of an app installed from another computer, see
    /// `NewDevice::app`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) app: Option<String>,
    pub(crate) id: DeviceId,
    pub(crate) name: String,
    pub(crate) public_key: PublicKey,
    pub(crate) created_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_seen_ms: Option<u64>,
    /// The browser's push subscription, opaque to the store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) push: Option<serde_json::Value>,
    /// The iOS companion's APNs tokens (`apns::Registration`), opaque to
    /// the store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) apns: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    devices: Vec<DeviceRecord>,
}

#[derive(Debug)]
pub(crate) struct Devices {
    path: PathBuf,
    records: Vec<DeviceRecord>,
    /// The last-seen time each device has on disk, to bound touch writes.
    stored_seen: HashMap<DeviceId, u64>,
}

impl Devices {
    /// Reads `state_dir/devices.json`; a missing file is an empty store.
    /// Blocking.
    pub(crate) fn load(state_dir: &Path) -> Result<Self> {
        let path = state_dir.join(FILE_NAME);
        let records = match fs::read(&path) {
            Ok(bytes) => {
                serde_json::from_slice::<StoreFile>(&bytes)
                    .map_err(|source| Error::StoreFormat {
                        path: path.clone(),
                        source,
                    })?
                    .devices
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(source) => return Err(store_error(StoreOperation::Read, &path, source)),
        };
        let stored_seen = records
            .iter()
            .filter_map(|record| Some((record.id.clone(), record.last_seen_ms?)))
            .collect();
        Ok(Self {
            path,
            records,
            stored_seen,
        })
    }

    #[cfg(test)]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }

    pub(crate) fn get(&self, id: &DeviceId) -> Option<&DeviceRecord> {
        self.records.iter().find(|record| &record.id == id)
    }

    /// Stores an approved device. Blocking.
    pub(crate) fn add(&mut self, device: NewDevice, now_ms: u64) -> Result<()> {
        self.records.push(DeviceRecord {
            app: device.app,
            id: device.id,
            name: device.name,
            public_key: device.public_key,
            created_ms: now_ms,
            last_seen_ms: None,
            push: None,
            apns: None,
        });
        self.save_or_undo(|devices| {
            devices.records.pop();
        })
    }

    /// The origins of apps installed from other computers that paired
    /// devices use.
    pub(crate) fn apps(&self) -> Vec<String> {
        let mut apps: Vec<String> = self
            .records
            .iter()
            .filter_map(|record| record.app.clone())
            .collect();
        apps.sort_unstable();
        apps.dedup();
        apps
    }

    /// The devices for Herdr UI, oldest first; `connected` says whether one
    /// has a stream open now.
    pub(crate) fn list(&self, connected: impl Fn(&DeviceId) -> bool) -> Vec<control::Device> {
        self.records
            .iter()
            .map(|record| control::Device {
                id: record.id.clone(),
                name: record.name.clone(),
                created_ms: record.created_ms,
                last_seen_ms: record.last_seen_ms,
                push: record.push.is_some(),
                connected: connected(&record.id),
            })
            .collect()
    }

    /// Marks a device seen now, writing the store only when its stored time
    /// is a minute old. Blocking when it writes.
    pub(crate) fn touch(&mut self, id: &DeviceId, now_ms: u64) -> Result<()> {
        let record = self.record_mut(id)?;
        record.last_seen_ms = Some(now_ms);
        let stale = self
            .stored_seen
            .get(id)
            .is_none_or(|stored| now_ms.saturating_sub(*stored) >= TOUCH_PERSIST_MS);
        if !stale {
            return Ok(());
        }
        self.save()?;
        self.stored_seen.insert(id.clone(), now_ms);
        Ok(())
    }

    /// Gives a device a new key, as its Home Screen app takes over from the
    /// browser. The browser's push subscription goes with the old key.
    /// Blocking.
    pub(crate) fn rekey(&mut self, id: &DeviceId, public_key: PublicKey) -> Result<()> {
        let record = self.record_mut(id)?;
        let old = (
            std::mem::replace(&mut record.public_key, public_key),
            record.push.take(),
        );
        let id = id.clone();
        self.save_or_undo(move |devices| {
            if let Some(record) = devices.records.iter_mut().find(|record| record.id == id) {
                (record.public_key, record.push) = old;
            }
        })
    }

    /// Forgets a device; `false` when there was none. Blocking.
    pub(crate) fn revoke(&mut self, id: &DeviceId) -> Result<bool> {
        let Some(index) = self.records.iter().position(|record| &record.id == id) else {
            return Ok(false);
        };
        let record = self.records.remove(index);
        self.save_or_undo(move |devices| devices.records.insert(index, record))?;
        self.stored_seen.remove(id);
        Ok(true)
    }

    /// Replaces a device's push subscription, or drops it with `None`.
    /// Blocking.
    pub(crate) fn set_push(
        &mut self,
        id: &DeviceId,
        push: Option<serde_json::Value>,
    ) -> Result<()> {
        let record = self.record_mut(id)?;
        let previous = std::mem::replace(&mut record.push, push);
        let id = id.clone();
        self.save_or_undo(move |devices| {
            if let Ok(record) = devices.record_mut(&id) {
                record.push = previous;
            }
        })
    }

    /// Replaces a device's APNs registration, or drops it with `None`.
    /// Blocking.
    pub(crate) fn set_apns(
        &mut self,
        id: &DeviceId,
        apns: Option<serde_json::Value>,
    ) -> Result<()> {
        let record = self.record_mut(id)?;
        let previous = std::mem::replace(&mut record.apns, apns);
        let id = id.clone();
        self.save_or_undo(move |devices| {
            if let Ok(record) = devices.record_mut(&id) {
                record.apns = previous;
            }
        })
    }

    /// Forgets devices not seen (or, never seen, not paired) for `idle_ms`.
    /// Blocking when any expire.
    pub(crate) fn expire_idle(&mut self, now_ms: u64, idle_ms: u64) -> Result<Vec<DeviceId>> {
        let idle = |record: &DeviceRecord| {
            let seen = record.last_seen_ms.unwrap_or(record.created_ms);
            now_ms.saturating_sub(seen) >= idle_ms
        };
        if !self.records.iter().any(idle) {
            return Ok(Vec::new());
        }
        let previous = self.records.clone();
        self.records.retain(|record| !idle(record));
        self.save_or_undo(|devices| devices.records = previous.clone())?;
        let expired: Vec<DeviceId> = previous
            .into_iter()
            .filter(|record| idle(record))
            .map(|record| record.id)
            .collect();
        for id in &expired {
            self.stored_seen.remove(id);
        }
        Ok(expired)
    }

    fn record_mut(&mut self, id: &DeviceId) -> Result<&mut DeviceRecord> {
        self.records
            .iter_mut()
            .find(|record| &record.id == id)
            .ok_or(Error::UnknownDevice)
    }

    /// Saves, or restores the in-memory state with `undo` when the write
    /// fails, so memory never claims what the disk lacks.
    fn save_or_undo(&mut self, undo: impl FnOnce(&mut Self)) -> Result<()> {
        let result = self.save();
        if result.is_err() {
            undo(self);
        }
        result
    }

    fn save(&self) -> Result<()> {
        let content = serde_json::to_vec_pretty(&StoreFile {
            version: VERSION,
            devices: self.records.clone(),
        })
        .map_err(Error::Encode)?;
        write_private(&self.path, &content)
    }
}

fn store_error(operation: StoreOperation, path: &Path, source: io::Error) -> Error {
    Error::Store {
        operation,
        path: path.to_owned(),
        source,
    }
}

/// Replaces `path` with `content` through a temporary file in the same
/// directory, created 0600 on Unix. Windows has no mode bits; the file
/// inherits the private state directory's ACL.
pub(super) fn write_private(path: &Path, content: &[u8]) -> Result<()> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    fs::create_dir_all(parent)
        .map_err(|source| store_error(StoreOperation::CreateDirectory, parent, source))?;
    let temp = parent.join(format!(
        ".{name}-{}-{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temp)
        .map_err(|source| store_error(StoreOperation::CreateTemporary, path, source))?;
    let written = file
        .write_all(content)
        .map_err(|source| store_error(StoreOperation::Write, &temp, source))
        .and_then(|()| {
            file.sync_all()
                .map_err(|source| store_error(StoreOperation::Sync, &temp, source))
        })
        .and_then(|()| {
            drop(file);
            fs::rename(&temp, path)
                .map_err(|source| store_error(StoreOperation::Replace, path, source))
        });
    if written.is_err() {
        // Best effort: the error that matters is the one being returned.
        let _ = fs::remove_file(&temp);
        return written;
    }
    // A directory cannot be opened for fsync on Windows, where the rename is
    // already ordered by the filesystem.
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| store_error(StoreOperation::Sync, parent, source))?;
    Ok(())
}
