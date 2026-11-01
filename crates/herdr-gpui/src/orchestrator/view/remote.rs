//! Azure DevOps beyond the open checkouts. Each organization a checkout's
//! remote names gets one more project, listing every pull request its reader
//! created there, and its catalog of projects and repositories, which the
//! Projects menu browses. A repository picked there is listed for the rest
//! of the session without a checkout, until picked again or closed.

use super::{
    OrchestratorView,
    repo::{Repo, remote_key},
};
use crate::orchestrator::{Provider, RemoteScope, SourceKey, azure::organization::Catalog};
use std::sync::Arc;

/// The most organizations a view reads its reader's pull requests from.
const MAX_ORGANIZATIONS: usize = 8;
/// The most repositories a view lists without a checkout.
pub(super) const MAX_BROWSED: usize = 16;

/// What the Projects menu shows of one organization.
pub(super) struct Organization<'a> {
    pub(super) name: &'a str,
    /// The repository of its own pull requests, whose worker reads the
    /// catalog and is refreshed to read it again.
    pub(super) repo: usize,
    pub(super) catalog: Option<&'a Arc<Catalog>>,
}

impl OrchestratorView {
    /// The Azure DevOps organizations the checkouts' remotes name, in the
    /// order the checkouts are listed.
    pub(super) fn known_organizations(&self) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        let remotes = self
            .repos
            .iter()
            .filter(|repo| repo.remote.is_none())
            .filter_map(|repo| repo.snapshot.repo.as_ref()?.remote.as_ref());
        for remote in remotes {
            if remote.source.provider != Provider::Azure {
                continue;
            }
            let Some((organization, _)) = remote.source.repository.split_once('/') else {
                continue;
            };
            if !found.iter().any(|known| known == organization) {
                found.push(organization.to_owned());
            }
        }
        found.truncate(MAX_ORGANIZATIONS);
        found
    }

    /// Starts the reader's pull requests of each organization newly known.
    /// An organization stays for the session once known, so a checkout
    /// that is briefly unreadable does not drop what was browsed.
    pub(super) fn sync_remotes(&mut self) -> bool {
        let mut added = false;
        for organization in self.known_organizations() {
            let scope = RemoteScope::Mine { organization };
            let key = remote_key(&scope);
            if !self.repos.iter().any(|repo| repo.key == key) {
                self.repos.push(Repo::start_remote(scope));
                added = true;
            }
        }
        if added {
            self.refresh_rows();
        }
        added
    }

    /// The organizations the Projects menu lists, with their catalogs.
    pub(super) fn organizations(&self) -> Vec<Organization<'_>> {
        self.repos
            .iter()
            .enumerate()
            .filter_map(|(index, repo)| match &repo.remote {
                Some(RemoteScope::Mine { organization }) => Some(Organization {
                    name: organization,
                    repo: index,
                    catalog: repo.snapshot.catalog.as_ref(),
                }),
                _ => None,
            })
            .collect()
    }

    /// Whether `source` is listed through the Projects menu.
    pub(super) fn browsed(&self, source: &SourceKey) -> bool {
        let key = remote_key(&RemoteScope::Repository(source.clone()));
        self.repos.iter().any(|repo| repo.key == key)
    }

    /// Lists `source` without a checkout, or stops listing it when it is.
    /// Past [`MAX_BROWSED`], nothing more is added.
    pub(super) fn toggle_browsed(&mut self, source: SourceKey) {
        let scope = RemoteScope::Repository(source);
        let key = remote_key(&scope);
        if self.repos.iter().any(|repo| repo.key == key) {
            self.stop_browsing(&key);
            return;
        }
        let browsed = self
            .repos
            .iter()
            .filter(|repo| matches!(repo.remote, Some(RemoteScope::Repository(_))))
            .count();
        if browsed >= MAX_BROWSED {
            return;
        }
        self.repos.push(Repo::start_remote(scope));
        self.refresh_rows();
    }

    /// Stops listing the browsed repository `key`; its worker ends with it.
    pub(super) fn stop_browsing(&mut self, key: &str) {
        let active = self.repo().map(|repo| repo.key.clone());
        self.repos.retain(|repo| {
            repo.key != key || !matches!(repo.remote, Some(RemoteScope::Repository(_)))
        });
        self.active = active
            .and_then(|key| self.repos.iter().position(|repo| repo.key == key))
            .unwrap_or(0);
        self.projects.remove(key);
        if self
            .detail
            .as_ref()
            .is_some_and(|detail| self.item(&detail.key).is_none())
        {
            self.detail = None;
        }
        self.refresh_rows();
    }

    /// Reads organization `repo`'s catalog and pull requests again.
    pub(super) fn refresh_organization(&mut self, repo: usize) {
        if let Some(service) = self.repos.get(repo).and_then(|repo| repo.service.as_ref()) {
            service.refresh(None);
        }
    }
}
