//! Revocations in a file, on blocking threads so a slow disk does not hold
//! the runtime.

use super::{Result, Revoked, Store};
use crate::credential::DeviceId;
use rusqlite::{Connection, params};
use std::{
    path::Path,
    sync::{Arc, Mutex, PoisonError},
};

const SCHEMA: &str = "
    PRAGMA journal_mode = WAL;
    CREATE TABLE IF NOT EXISTS revoked (
        id TEXT PRIMARY KEY NOT NULL,
        until_s INTEGER NOT NULL
    ) WITHOUT ROWID;
";

#[derive(Clone)]
pub struct Sqlite {
    connection: Arc<Mutex<Connection>>,
}

impl Sqlite {
    /// Opens or creates the database at `path`. Blocking file I/O.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.execute_batch(SCHEMA)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    async fn with<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> Result<T> {
        let connection = self.connection.clone();
        let result = tokio::task::spawn_blocking(move || {
            work(&connection.lock().unwrap_or_else(PoisonError::into_inner))
        })
        .await?;
        Ok(result?)
    }
}

impl Store for Sqlite {
    async fn revoke(&self, what: &Revoked, until_s: u64) -> Result<()> {
        let key = what.key();
        let until_s = i64::try_from(until_s).unwrap_or(i64::MAX);
        self.with(move |connection| {
            connection.execute(
                "INSERT INTO revoked (id, until_s) VALUES (?1, ?2)
                 ON CONFLICT (id) DO UPDATE SET until_s = max(until_s, ?2)",
                params![key, until_s],
            )
        })
        .await?;
        Ok(())
    }

    async fn is_revoked(&self, device: DeviceId, handle: &str) -> Result<bool> {
        let device = Revoked::Device(device).key();
        let handle = Revoked::Handle(handle.to_owned()).key();
        self.with(move |connection| {
            connection.query_row(
                "SELECT EXISTS (SELECT 1 FROM revoked WHERE id IN (?1, ?2))",
                params![device, handle],
                |row| row.get(0),
            )
        })
        .await
    }

    async fn purge(&self, now_s: u64) -> Result<usize> {
        let now_s = i64::try_from(now_s).unwrap_or(i64::MAX);
        self.with(move |connection| {
            connection.execute("DELETE FROM revoked WHERE until_s <= ?1", params![now_s])
        })
        .await
    }
}
