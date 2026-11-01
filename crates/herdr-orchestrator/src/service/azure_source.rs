//! Reading an Azure DevOps source with the token looked up once, and looked
//! up again when Azure DevOps refuses it: an Azure CLI token lasts about an
//! hour, and the next one comes from the same sign-in.

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
        if listing.viewer.is_some() {
            self.snapshot.viewer.clone_from(&listing.viewer);
        }
        // This source's own pull requests, as the listing matched them.
        let prefix = format!("{}:", key.canonical());
        let mut authored = (*self.snapshot.authored).clone();
        authored.retain(|item| !item.starts_with(&prefix));
        authored.extend(listing.authored.iter().cloned());
        self.snapshot.authored = std::sync::Arc::new(authored);
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
        let looked_up = self.azure_token.is_some();
        if !looked_up {
            self.look_up_azure_token();
        }
        match self.read_azure(key, previous) {
            Err(Error::Azure(azure::Error::Authentication)) if looked_up => {
                self.look_up_azure_token();
                self.read_azure(key, previous)
            }
            result => result,
        }
    }

    fn look_up_azure_token(&mut self) {
        let cancelled = &self.cancelled;
        self.azure_token = Some(azure::token::lookup(&|| cancelled.load(Ordering::Relaxed)));
    }

    fn read_azure(
        &mut self,
        key: &SourceKey,
        previous: Option<&Checkpoint>,
    ) -> Result<azure::Listing> {
        let token = self.azure_token.as_ref().and_then(Option::as_ref);
        let cancelled = &self.cancelled;
        azure::sync(
            key,
            token,
            previous,
            &|| cancelled.load(Ordering::Relaxed),
            &mut self.cooldown,
        )
    }
}
