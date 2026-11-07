//! What the relay keeps: revocations, and nothing else. No token or payload
//! may ever reach a store, which a test checks.
//!
//! Which one runs is the operator's choice:
//!
//! - [`Memory`], the default, needs no disk or database. A restart forgets
//!   revocations, so a revoked handle works again until its credential
//!   expires, at most thirty days on. Pushes keep working through a restart:
//!   each one carries its own sealed handle.
//! - [`Sqlite`] keeps them in a file.
//! - `Postgres`, with the `postgres` feature, keeps them in a database that
//!   several relays can share.

use crate::credential::DeviceId;
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

mod error;
#[cfg(feature = "postgres")]
mod postgres;
mod sqlite;

pub use error::{Error, Result};
#[cfg(feature = "postgres")]
pub use postgres::Postgres;
pub use sqlite::Sqlite;

/// The most revocations [`Memory`] holds. Past this it refuses a new one, so
/// the caller hears it failed rather than the relay running out of memory.
const MAX_MEMORY: usize = 1 << 20;

/// What a revocation covers. Handle and device ids share one table, so each
/// is stored under its own prefix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Revoked {
    Device(DeviceId),
    Handle(String),
}

impl Revoked {
    fn key(&self) -> String {
        match self {
            Self::Device(device) => {
                let hex: String = device.iter().map(|byte| format!("{byte:02x}")).collect();
                format!("device:{hex}")
            }
            Self::Handle(id) => format!("handle:{id}"),
        }
    }
}

/// Where revocations live. An error means the answer is unknown: the relay
/// then refuses the push, since the revocation may be in force.
pub trait Store: Clone + Send + Sync + 'static {
    /// Revokes `what` until `until_s`, after which its credential has
    /// expired anyway and the record can go. Revoking twice keeps the later
    /// date.
    fn revoke(&self, what: &Revoked, until_s: u64) -> impl Future<Output = Result<()>> + Send;

    /// Whether the device or the handle has been revoked.
    fn is_revoked(
        &self,
        device: DeviceId,
        handle: &str,
    ) -> impl Future<Output = Result<bool>> + Send;

    /// Drops revocations whose credentials have expired, returning how many.
    fn purge(&self, now_s: u64) -> impl Future<Output = Result<usize>> + Send;
}

/// Revocations for as long as the process runs.
#[derive(Clone, Debug, Default)]
pub struct Memory {
    revoked: Arc<Mutex<HashMap<String, u64>>>,
}

impl Memory {
    fn revoked(&self) -> MutexGuard<'_, HashMap<String, u64>> {
        self.revoked.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Store for Memory {
    async fn revoke(&self, what: &Revoked, until_s: u64) -> Result<()> {
        let key = what.key();
        let mut revoked = self.revoked();
        if revoked.len() >= MAX_MEMORY && !revoked.contains_key(&key) {
            return Err(Error::Full);
        }
        let until = revoked.entry(key).or_default();
        *until = until_s.max(*until);
        Ok(())
    }

    async fn is_revoked(&self, device: DeviceId, handle: &str) -> Result<bool> {
        let revoked = self.revoked();
        Ok(revoked.contains_key(&Revoked::Device(device).key())
            || revoked.contains_key(&Revoked::Handle(handle.to_owned()).key()))
    }

    async fn purge(&self, now_s: u64) -> Result<usize> {
        let mut revoked = self.revoked();
        let before = revoked.len();
        revoked.retain(|_, until_s| *until_s > now_s);
        Ok(before - revoked.len())
    }
}
