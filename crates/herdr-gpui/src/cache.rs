//! The app's cache database, `cache/cache.sqlite3` in its state directory,
//! in a folder of its own that only this user may enter: what
//! the last run found that spans repositories or comes before one is known,
//! such as which repository a checkout holds and the Security section's
//! read, so a restart shows it at once while the sections' own timers read
//! it again. Each kind keeps one JSON value per key; a value that no longer
//! decodes is a miss, not a failure. Every call blocks on disk I/O, so it
//! runs on a worker or background thread, never on the UI thread.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const FILE: &str = "cache.sqlite3";
/// How long a write waits for another window's or process's transaction.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS entries (
    kind TEXT NOT NULL,
    key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    saved_at TEXT NOT NULL,
    PRIMARY KEY (kind, key)
)";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Cache database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Cannot store a cache entry: {0}")]
    Json(#[from] serde_json::Error),
    #[error("No state directory is available for the cache")]
    NoStateDirectory,
    #[error(transparent)]
    Private(#[from] crate::private_file::Error),
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

/// What an entry holds; each names its own keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A checkout's repository, keyed by its host and folder.
    Repository,
    /// The Security section's read, keyed by the logins it was read as.
    Security,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::Security => "security",
        }
    }
}

/// An entry and when it was saved.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Entry<T> {
    pub(crate) value: T,
    pub(crate) saved_at: chrono::DateTime<chrono::Utc>,
}

pub(crate) struct Cache {
    connection: Connection,
}

impl Cache {
    pub(crate) fn open_at(path: &Path) -> Result<Self> {
        crate::private_file::prepare(path)?;
        let connection = Connection::open(path)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        connection.query_row("PRAGMA journal_mode = WAL", [], |row| {
            row.get::<_, String>(0)
        })?;
        connection.execute(SCHEMA, [])?;
        Ok(Self { connection })
    }

    /// `key`'s entry of `kind`, or `None` when there is none or it no
    /// longer decodes as `T`.
    pub(crate) fn get<T: DeserializeOwned>(
        &self,
        kind: Kind,
        key: &str,
    ) -> Result<Option<Entry<T>>> {
        let row: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT value_json, saved_at FROM entries WHERE kind = ?1 AND key = ?2",
                params![kind.name(), key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(row.and_then(|(json, saved_at)| {
            Some(Entry {
                value: serde_json::from_str(&json).ok()?,
                saved_at: chrono::DateTime::parse_from_rfc3339(&saved_at)
                    .ok()?
                    .to_utc(),
            })
        }))
    }

    pub(crate) fn put(&self, kind: Kind, key: &str, value: &impl Serialize) -> Result<()> {
        let json = serde_json::to_string(value)?;
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.connection.execute(
            "INSERT INTO entries (kind, key, value_json, saved_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (kind, key) DO UPDATE SET value_json = ?3, saved_at = ?4",
            params![kind.name(), key, json, now],
        )?;
        Ok(())
    }

    pub(crate) fn remove(&self, kind: Kind, key: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM entries WHERE kind = ?1 AND key = ?2",
            params![kind.name(), key],
        )?;
        Ok(())
    }
}

/// The cache's file in the app's state directory.
pub(crate) fn path() -> Result<PathBuf> {
    let state = crate::preferences::state_dir().ok_or(Error::NoStateDirectory)?;
    Ok(path_in(&state))
}

pub(crate) fn path_in(state: &Path) -> PathBuf {
    state.join("cache").join(FILE)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
