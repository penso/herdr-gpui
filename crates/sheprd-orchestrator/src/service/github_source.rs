//! Reading a GitHub source: its issues and pull requests, then its checks and
//! reviews when they may have moved. A listing the database does not keep
//! (capped, or the database is newer) is kept in memory with its checkpoint,
//! so the next sync reads only its changes as a stored one does.

use super::{
    Error, Result, SourceKey, SyncState, Worker, github,
    schedule::{self, ChecksRead},
};
use crate::Checkpoint;
use chrono::Utc;
use sheprd_github::Tokens;
use std::{
    sync::{Arc, atomic::Ordering},
    time::Instant,
};

/// A GitHub listing shown but not stored, and where its next sync continues.
#[derive(Debug)]
pub(super) struct Remembered {
    checkpoint: Checkpoint,
    /// Whether the listing was complete: a capped one stays capped through
    /// its incremental syncs (see [`schedule::complete`]).
    complete: bool,
}

impl Worker {
    pub(super) fn github_listing(&mut self, key: &SourceKey, tokens: &Tokens) -> Result<SyncState> {
        let Some(store) = self.store.as_mut() else {
            return Ok(SyncState::Unsupported);
        };
        let writable = store.access() == super::Access::ReadWrite;
        let owner = key
            .repository
            .split_once('/')
            .map_or(key.repository.as_str(), |(owner, _)| owner);
        let token = tokens.for_owner(owner);
        // A listing shown from memory continues from its own checkpoint;
        // else what the last complete sync stored, which a read-only store
        // cannot give.
        let remembered = self.uncached.get(key).zip(self.remembered.get(key));
        let continued_complete = remembered.map(|(_, remembered)| remembered.complete);
        let (previous, cached) = match remembered {
            Some((items, remembered)) => (Some(remembered.checkpoint.clone()), Some(items.clone())),
            None => {
                let previous = store.checkpoint(key)?;
                let cached = match (&previous, writable) {
                    (Some(_), true) => Some(
                        store
                            .items()?
                            .into_iter()
                            .filter(|item| item.key.source == *key)
                            .collect(),
                    ),
                    _ => None,
                };
                (previous, cached)
            }
        };
        // Only while the checkpoint is still the one this worker wrote:
        // agent-launcher shares the database and may have stored an older
        // listing since.
        let resume = self.resume.get(key).copied().filter(|resume| {
            previous
                .as_ref()
                .and_then(|checkpoint| checkpoint.updated_at)
                == Some(resume.oldest())
        });
        let cancelled = &self.cancelled;
        let listing = github::sync(
            key,
            token,
            previous.as_ref(),
            (cached, resume),
            &|| cancelled.load(Ordering::Relaxed),
            &mut self.cooldown,
        )?;
        let count = listing.items.len();
        let complete = schedule::complete(listing.complete, listing.full, continued_complete);
        self.read_checks(key, token, &listing.items)?;
        self.resume.insert(key.clone(), listing.resume);
        if !complete || !writable {
            self.remembered.insert(
                key.clone(),
                Remembered {
                    checkpoint: listing.checkpoint,
                    complete,
                },
            );
            self.uncached.insert(key.clone(), listing.items);
            return Ok(if complete {
                SyncState::Synced {
                    at: Utc::now(),
                    count,
                }
            } else {
                SyncState::Capped { count }
            });
        }
        if let Some(store) = self.store.as_mut() {
            store.replace_items(key, &listing.items, &listing.checkpoint)?;
        }
        self.remembered.remove(key);
        self.uncached.remove(key);
        Ok(SyncState::Synced {
            at: Utc::now(),
            count,
        })
    }

    /// Reads the open pull requests' reviews and checks when they may have
    /// changed (see [`schedule::checks_due`]). Failing to read them keeps the
    /// last ones rather than failing the listing, and the next sync tries
    /// again.
    fn read_checks(
        &mut self,
        key: &SourceKey,
        token: &secrecy::SecretString,
        items: &[crate::Item],
    ) -> Result<()> {
        let open = schedule::open_pulls(items);
        let now = Instant::now();
        if !schedule::checks_due(
            self.checks.get(key),
            &open,
            &key.repository,
            &self.snapshot.pr_status,
            now,
        ) {
            return Ok(());
        }
        let cancelled = &self.cancelled;
        let statuses = crate::review::fetch(&key.repository, |query, variables| {
            sheprd_github::graphql(
                "orchestrator_checks",
                token,
                query,
                variables,
                crate::review::timeout(),
                || cancelled.load(Ordering::Relaxed),
                &mut self.cooldown,
            )
            .map_err(|error| match error {
                sheprd_github::Error::PullRequest(
                    sheprd_github::pull_request::Error::Cancelled,
                ) => Error::Cancelled,
                error => Error::Github(Box::new(error)),
            })
        });
        let statuses = match statuses {
            Ok(statuses) => statuses,
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(error) => {
                tracing::warn!(%error, "Cannot read pull request checks");
                return Ok(());
            }
        };
        self.checks
            .insert(key.clone(), ChecksRead { at: now, open });
        if let Some(store) = self.store.as_mut()
            && store.access() == super::Access::ReadWrite
            && let Err(error) = store.replace_pr_statuses(&key.repository, &statuses)
        {
            tracing::warn!(%error, "Cannot keep pull request checks");
        }
        let mut all = (*self.snapshot.pr_status).clone();
        all.retain(|(repository, _), _| *repository != key.repository);
        all.extend(
            statuses
                .into_iter()
                .map(|(number, status)| ((key.repository.clone(), number), status)),
        );
        self.snapshot.pr_status = Arc::new(all);
        Ok(())
    }
}
