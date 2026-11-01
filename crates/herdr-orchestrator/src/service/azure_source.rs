//! Reading an Azure DevOps source with its organization's token, looked up
//! once and looked up again when Azure DevOps refuses it: an Azure CLI token
//! lasts about an hour, and the next one comes from the same sign-in, and a
//! token pasted in the account menu may have replaced the refused one.

use super::{Error, Result, SourceKey, SyncState, Worker, azure};
use crate::Checkpoint;
use chrono::Utc;
use std::sync::atomic::Ordering;

impl Worker {
    /// Syncs Azure DevOps source `key`, storing a complete listing when the
    /// database is `writable` and showing any other without storing it.
    pub(super) fn sync_azure(&mut self, key: &SourceKey, writable: bool) -> Result<SyncState> {
        let Some(store) = self.store.as_mut() else {
            return Ok(SyncState::Unsupported);
        };
        let previous = store.checkpoint(key)?;
        let listing = self.azure_listing(key, previous.as_ref())?;
        let Some(store) = self.store.as_mut() else {
            return Ok(SyncState::Unsupported);
        };
        let count = listing.items.len();
        if !listing.complete || !writable {
            self.uncached.insert(key.clone(), listing.items);
            return Ok(if listing.complete {
                SyncState::Synced {
                    at: Utc::now(),
                    count,
                }
            } else {
                SyncState::Capped { count }
            });
        }
        store.replace_items(key, &listing.items, &listing.checkpoint)?;
        self.uncached.remove(key);
        Ok(SyncState::Synced {
            at: Utc::now(),
            count,
        })
    }

    /// Lists Azure DevOps source `key`, continuing from `previous`.
    pub(super) fn azure_listing(
        &mut self,
        key: &SourceKey,
        previous: Option<&Checkpoint>,
    ) -> Result<azure::Listing> {
        let organization = azure::Repository::organization(key)
            .unwrap_or_default()
            .to_ascii_lowercase();
        let looked_up = self
            .azure_token
            .as_ref()
            .is_some_and(|(looked_up, _)| *looked_up == organization);
        if !looked_up {
            self.look_up_azure_token(&organization);
        }
        match self.read_azure(key, previous) {
            Err(Error::Azure(azure::Error::Authentication)) if looked_up => {
                self.look_up_azure_token(&organization);
                self.read_azure(key, previous)
            }
            result => result,
        }
    }

    fn look_up_azure_token(&mut self, organization: &str) {
        let cancelled = &self.cancelled;
        let token = azure::token::lookup(organization, self.request.azure_store, &|| {
            cancelled.load(Ordering::Relaxed)
        });
        self.azure_token = Some((organization.to_owned(), token));
    }

    fn read_azure(
        &mut self,
        key: &SourceKey,
        previous: Option<&Checkpoint>,
    ) -> Result<azure::Listing> {
        let found = self
            .azure_token
            .as_ref()
            .and_then(|(_, found)| found.as_ref());
        let cancelled = &self.cancelled;
        azure::sync(
            key,
            found.map(|found| &found.token),
            previous,
            &|| cancelled.load(Ordering::Relaxed),
            &mut self.cooldown,
        )
        .map_err(|error| match (error, found) {
            (Error::Azure(error), Some(found)) => Error::Azure(error.from_source(found.source)),
            (error, _) => error,
        })
    }
}
