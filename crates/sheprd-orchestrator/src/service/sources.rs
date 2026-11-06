//! Which sources a repository has, which this build reads, and the GitLab
//! CLI's say in whether a remote is a GitLab project.

use super::{Item, Provider, RepoInfo, SourceKey, SourceStatus, SyncState, Worker, gitlab};

impl SyncState {
    /// How many items the source's last sync reported, kept while it syncs
    /// again; `None` before one has, or after one failed.
    pub fn count(&self) -> Option<usize> {
        match self {
            Self::Synced { count, .. } | Self::Capped { count } => Some(*count),
            Self::Syncing { count } => *count,
            Self::SignIn | Self::Unsupported | Self::Failed(_) => None,
        }
    }
}

impl Worker {
    /// Marks `key` syncing, keeping the count its last sync reported.
    pub(super) fn set_syncing(&mut self, key: &SourceKey) {
        if let Some(status) = self.snapshot.sources.iter_mut().find(|s| s.key == *key) {
            status.state = SyncState::Syncing {
                count: status.state.count(),
            };
        }
    }
}

/// Makes `info`'s remote a GitLab source when glab knows its host; returns
/// whether it changed. `cli` reads glab's config, only when needed.
pub(super) fn recognize(
    info: &mut RepoInfo,
    cli: impl FnOnce() -> Vec<gitlab::cli_hosts::CliHost>,
) -> bool {
    info.remote
        .as_mut()
        .is_some_and(|remote| gitlab::recognize(&mut remote.source, cli))
}

/// Drops beads of any source but `info`'s. A project keeps them in its
/// database after its source moves, as a `.beads/redirect` added later moves
/// it, and they would list beside the ones read now. Before the repository
/// is known, what was last listed stays.
pub(super) fn keep_current_beads(items: &mut Vec<Item>, info: Option<&RepoInfo>) {
    let Some(info) = info else {
        return;
    };
    let current = info.beads_source();
    items.retain(|item| {
        item.key.source.provider != Provider::Beads || Some(&item.key.source) == current.as_ref()
    });
}

/// Each source of `info`, syncing when this build can read it.
pub(super) fn statuses(info: &RepoInfo) -> Vec<SourceStatus> {
    sources(info)
        .into_iter()
        .map(|(key, supported)| SourceStatus {
            key,
            state: if supported {
                SyncState::Syncing { count: None }
            } else {
                SyncState::Unsupported
            },
        })
        .collect()
}

/// The sources a repository has, and whether this build can read each.
pub fn sources(info: &RepoInfo) -> Vec<(SourceKey, bool)> {
    let remote = info.remote.as_ref().map(|remote| {
        let supported = match remote.source.provider {
            Provider::Github => remote.source.host == "github.com",
            Provider::Gitlab => true,
            Provider::Beads => false,
        };
        (remote.source.clone(), supported)
    });
    remote
        .into_iter()
        .chain(info.beads_source().map(|key| (key, true)))
        .collect()
}
