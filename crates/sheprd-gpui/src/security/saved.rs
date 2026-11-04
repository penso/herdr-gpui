//! The last read, kept in the app's cache (`crate::cache`) under the logins
//! it was read as and the checkouts it read, so a restart lists a window's
//! alerts and reports at once while the first read of the run is still out,
//! and never another window's. Only what the list shows is kept:
//! names, alerts, and report titles and metadata, never a report's
//! description, which is not read for the list at all, and never a token.
//! A repository's alerts or reports that failed to read keep what the
//! cache last held for them; the other half is kept fresh.
//!
//! Until the run's own read lands, a restored repository has no account to
//! read an item's page with, and no checkouts to start an agent in.

use super::{Alerts, Error, Repo, Reports};
use crate::{
    cache::{Cache, Kind},
    orchestrator::RepoSpec,
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::SystemTime};

/// One repository as the cache keeps it: each half once it was read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Saved {
    name: String,
    host: String,
    alerts: Option<Alerts>,
    reports: Option<Reports>,
}

impl Saved {
    /// `repo`'s read, each half that failed taken from `before`.
    fn of(repo: &Repo, before: Option<&Saved>) -> Option<Self> {
        let alerts = repo
            .alerts
            .as_ref()
            .ok()
            .cloned()
            .or_else(|| before.and_then(|kept| kept.alerts.clone()));
        let reports = repo
            .reports
            .as_ref()
            .ok()
            .cloned()
            .or_else(|| before.and_then(|kept| kept.reports.clone()));
        (alerts.is_some() || reports.is_some()).then(|| Self {
            name: repo.name.clone(),
            host: repo.host.clone(),
            alerts,
            reports,
        })
    }

    fn into_repo(self) -> Repo {
        let unread = || Arc::new(Error::Unread);
        Repo {
            name: self.name,
            host: self.host,
            alerts: self.alerts.ok_or_else(unread),
            reports: self.reports.ok_or_else(unread),
            account: None,
            checkouts: Vec::new(),
        }
    }
}

/// The cache's key for a read as `accounts`, sorted, of the checkouts
/// `specs` name.
pub(super) fn key(accounts: &[String], specs: &[RepoSpec]) -> String {
    let mut checkouts: Vec<&str> = specs.iter().map(|spec| spec.key.as_str()).collect();
    checkouts.sort_unstable();
    checkouts.dedup();
    format!("{}\n\n{}", accounts.join("\n"), checkouts.join("\n"))
}

/// What the cache holds under `key`, and when it was read.
pub(super) fn restore(cache: &Cache, key: &str) -> Option<(Vec<Repo>, SystemTime)> {
    let entry = cache
        .get::<Vec<Saved>>(Kind::Security, key)
        .inspect_err(|error| tracing::debug!(%error, "Cannot read the cached Security read"))
        .ok()??;
    let repos = entry.value.into_iter().map(Saved::into_repo).collect();
    Some((repos, entry.saved_at.into()))
}

/// Keeps `repos` under `key`. A half that failed to read keeps what was
/// kept for it before; a repository no longer read is dropped.
pub(super) fn save(cache: &Cache, key: &str, repos: &[Repo]) {
    let before = cache
        .get::<Vec<Saved>>(Kind::Security, key)
        .ok()
        .flatten()
        .map(|entry| entry.value)
        .unwrap_or_default();
    let saved: Vec<Saved> = repos
        .iter()
        .filter_map(|repo| Saved::of(repo, before.iter().find(|kept| kept.name == repo.name)))
        .collect();
    if let Err(error) = cache.put(Kind::Security, key, &saved) {
        tracing::debug!(%error, "Cannot cache the Security read");
    }
}
