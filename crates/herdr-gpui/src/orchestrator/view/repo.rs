//! One repository a view lists: its worker, what the worker last read, and
//! the rows of it the list shows. A view lists every distinct repository the
//! window's workspaces hold on every connected host, so each keeps its own
//! worker and database, and an item's page and actions go to its own.

use super::rows::ItemRow;
use crate::orchestrator::{Error, Item, Request, Run, Service, Snapshot};
use std::sync::Arc;

/// A repository the window asks a view to list.
#[derive(Clone)]
pub(crate) struct RepoSpec {
    /// The host and repository it is, as the window tells repositories
    /// apart: the endpoint and the Git common directory.
    pub(crate) key: String,
    /// The host's name, as rows and pills show it.
    pub(crate) host: String,
    pub(crate) request: Request,
}

pub(crate) struct Repo {
    pub(super) key: String,
    pub(super) host: String,
    pub(super) service: Option<Service>,
    pub(crate) request: Request,
    pub(crate) snapshot: Snapshot,
    /// The runs the database records; the snapshot's runs add those found
    /// from branches.
    pub(crate) recorded_runs: Arc<Vec<Run>>,
    /// Items runs work on that are no longer listed, so those runs still
    /// open a page with their controls.
    pub(super) stand_ins: Vec<Item>,
    pub(super) issue_rows: Vec<ItemRow>,
    pub(super) pull_request_rows: Vec<ItemRow>,
    /// The signed-in GitHub login on its host, for "Mine".
    pub(super) login: Option<String>,
}

impl Repo {
    pub(super) fn start(spec: RepoSpec) -> Self {
        let (service, error) = match Service::start(spec.request.clone()) {
            Ok(service) => (Some(service), None),
            Err(error) => (None, Some(Arc::new(Error::Worker(error)))),
        };
        Self {
            key: spec.key,
            host: spec.host,
            service,
            request: spec.request,
            snapshot: Snapshot {
                error,
                ..Snapshot::default()
            },
            recorded_runs: Arc::default(),
            stand_ins: Vec::new(),
            issue_rows: Vec::new(),
            pull_request_rows: Vec::new(),
            login: None,
        }
    }

    /// What its pill and rows call it: the remote's repository name, else
    /// its main checkout's folder, else the folder it was found from.
    pub(super) fn name(&self) -> String {
        let info = self.snapshot.repo.as_ref();
        let named = info
            .and_then(|info| info.remote.as_ref())
            .map(|remote| remote.source.repository.as_str())
            .or_else(|| info.map(|info| info.main_root.as_str()))
            .unwrap_or(self.request.checkout.as_str());
        named
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(named)
            .to_owned()
    }

    /// The login GitHub is read as for this repository: the account that
    /// reads its owner, or the host's own before the remote is known.
    pub(super) fn reading_login(&self) -> Option<&str> {
        let owner = self
            .snapshot
            .repo
            .as_ref()
            .and_then(|info| info.remote.as_ref())
            .map(|remote| {
                let repository = remote.source.repository.as_str();
                repository
                    .split_once('/')
                    .map_or(repository, |(owner, _)| owner)
            });
        match (&self.request.token, owner) {
            (Some(tokens), Some(owner)) => Some(tokens.login_for(owner)),
            _ => self.login.as_deref(),
        }
    }

    /// Whether its folder turned out not to be in a Git repository: a
    /// workspace opened on any folder is no project, so it is left out
    /// rather than reported.
    pub(super) fn outside_git(&self) -> bool {
        matches!(self.snapshot.error.as_deref(), Some(Error::NotARepository))
    }

    /// The item `key` names here, listed or a stand-in.
    pub(super) fn item(&self, key: &str) -> Option<&Item> {
        self.snapshot
            .items
            .iter()
            .chain(&self.stand_ins)
            .find(|item| item.key.canonical() == key)
    }
}
