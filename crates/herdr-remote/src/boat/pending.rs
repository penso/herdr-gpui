//! Durable exactly-once creation intent. One account's requests are serialized
//! across windows and processes; a failed attempt keeps its key and exact body.
use super::{Error, Result, Settings, api::Create};
use crate::cloud::Value;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const LIMIT: u64 = 8192;
// Boat retains keys for 24 hours. Leave room for clock skew and HTTP retries;
// never replay an expired key, which would create a second billable sandbox.
const RETRY_WINDOW: u64 = 23 * 60 * 60;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Creation {
    pub(super) key: String,
    account: String,
    pub(super) name: String,
    pub(super) payload: Create,
    started_at: u64,
}

impl Creation {
    pub(super) fn new(settings: &Settings, name: &str, now: u64) -> Self {
        Self {
            key: uuid::Uuid::new_v4().hyphenated().to_string(),
            account: settings.account(),
            name: name.to_owned(),
            payload: Create {
                machine_type: settings.machine_type.clone(),
                environment: settings.environment.clone(),
                ttl_seconds: settings.ttl_seconds,
            },
            started_at: now,
        }
    }

    fn retryable(&self, now: u64) -> bool {
        now.checked_sub(self.started_at)
            .is_some_and(|age| age < RETRY_WINDOW)
    }
}

pub(super) fn directory() -> Result<PathBuf> {
    herdr_client::local_path::state_dir().ok_or_else(|| crate::cloud::Error::StateDirectory.into())
}

fn path(directory: &Path, settings: &Settings) -> PathBuf {
    let hash = Sha256::digest(settings.account().as_bytes());
    let hash: String = hash.iter().map(|byte| format!("{byte:02x}")).collect();
    directory.join(format!("boat-pending-{hash}.json"))
}

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::PendingIo {
        path: path.to_owned(),
        source,
    }
}

fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(Error::Clock)
}

fn read(path: &Path, settings: &Settings) -> Result<Option<Creation>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(path, error)),
    };
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io(path, error))?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::Field(Value::BoatPendingSize));
    }
    let creation: Creation = serde_json::from_slice(&bytes).map_err(Error::Json)?;
    if creation.account != settings.account()
        || uuid::Uuid::parse_str(&creation.key).is_err()
        || creation.name.is_empty()
        || creation.name.len() > 128
        || creation.name.chars().any(char::is_control)
    {
        return Err(Error::Field(Value::BoatPending));
    }
    Ok(Some(creation))
}

pub(super) fn overview(settings: &Settings) -> Result<Option<String>> {
    Ok(read(&path(&directory()?, settings), settings)?.map(|creation| creation.name))
}

pub(super) struct Journal {
    path: PathBuf,
    _lock: File,
}

impl Drop for Journal {
    fn drop(&mut self) {
        // A concurrent fork can inherit the descriptor before exec closes it.
        // Unlock explicitly: closing our descriptor alone would leave that
        // child's copy holding the lock and spuriously refuse an immediate retry.
        if let Err(error) = self._lock.unlock() {
            tracing::warn!(category = "boat_pending", %error, "Could not unlock Boat creation journal");
        }
    }
}

impl Journal {
    pub(super) fn open(directory: &Path, settings: &Settings) -> Result<Self> {
        fs::create_dir_all(directory).map_err(|error| io(directory, error))?;
        let path = path(directory, settings);
        let lock_path = path.with_extension("lock");
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| io(&lock_path, error))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(fs::TryLockError::WouldBlock) => return Err(Error::PendingBusy),
            Err(fs::TryLockError::Error(error)) => return Err(io(&lock_path, error)),
        }
        Ok(Self { path, _lock: lock })
    }

    pub(super) fn begin(&self, settings: &Settings, name: &str) -> Result<Creation> {
        self.begin_at(settings, name, now()?)
    }

    fn begin_at(&self, settings: &Settings, name: &str, now: u64) -> Result<Creation> {
        if let Some(creation) = read(&self.path, settings)? {
            return if creation.retryable(now) {
                Ok(creation)
            } else {
                Err(Error::PendingExpired)
            };
        }
        let creation = Creation::new(settings, name, now);
        let parent = self
            .path
            .parent()
            .ok_or(Error::Field(Value::BoatPendingPath))?;
        let mut file =
            tempfile::NamedTempFile::new_in(parent).map_err(|error| io(&self.path, error))?;
        serde_json::to_writer(&mut file, &creation).map_err(Error::Json)?;
        file.flush()
            .and_then(|()| file.as_file().sync_all())
            .map_err(|error| io(&self.path, error))?;
        file.persist(&self.path)
            .map_err(|error| io(&self.path, error.error))?;
        sync_parent(&self.path)?;
        Ok(creation)
    }

    pub(super) fn clear(&self) -> Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => sync_parent(&self.path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io(&self.path, error)),
        }
    }
}

fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|error| io(path, error))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(super) fn forget(settings: &Settings) -> Result<()> {
    Journal::open(&directory()?, settings)?.clear()
}

#[cfg(test)]
mod tests;
