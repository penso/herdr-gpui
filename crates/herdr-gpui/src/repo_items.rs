//! The repository behind a workspace, as a picker: its open pull requests and
//! issues, its branches without a checkout, the branch each one seeds, and the
//! note a created checkout carries about it. Read-only GitHub and Git work runs
//! off the UI thread; no local cache between openings.

mod branches;
mod context;
mod fetch;
mod lookup;
mod model;
mod remote;

#[cfg(test)]
mod tests;

pub(crate) use {
    branches::{Branch, list as list_branches},
    context::write_on as write_context,
    lookup::Lookup,
    model::{Item, Kind},
};

#[cfg(test)]
pub(crate) use model::issue_branch;

use crate::pull_request::Remote;

/// A lookup's checkout and the host that owns its paths travel together,
/// including through the worker queue and PR preparation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub host: crate::pull_request::Origin,
    pub input: crate::pull_request::Input,
}

/// The GitHub repository a workspace's pull requests are opened against: its
/// `upstream` remote in a fork checkout, otherwise `origin`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Origin {
    pub owner: String,
    pub repo: String,
    /// The remote that reaches this repository.
    pub remote: Remote,
    /// For an `upstream` listing, the owner of the fork behind `origin`, whose
    /// pull request heads are already this checkout's own branches.
    pub fork_owner: Option<String>,
}

impl From<crate::pull_request::Base> for Origin {
    fn from(base: crate::pull_request::Base) -> Self {
        Self {
            owner: base.owner,
            repo: base.repo,
            remote: base.remote,
            fork_owner: base.fork_owner,
        }
    }
}

impl Origin {
    pub(crate) fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}
