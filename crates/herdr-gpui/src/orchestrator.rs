//! The orchestrator: a repository's issues and pull requests, and the agent
//! runs dispatched for them.
//!
//! Its records live in its own per-repository SQLite database (`location`),
//! whose schema began as agent-launcher's; agent-launcher's file is copied
//! in the first time a repository opens. A restart shows what the database
//! and the app's cache (`crate::cache`) hold before reading anything again;
//! `docs/orchestrator.md` has the details.

mod actions;
mod beads;
mod error;
mod git;
mod github;
mod gitlab;
mod host;
mod location;
mod model;
mod naming;
mod prompt;
mod repo;
mod review;
mod section;
mod service;
mod store;
mod view;

pub(crate) use actions::{Action, DispatchRequest};
pub(crate) use error::{Error, Result};
pub(crate) use host::{LiveCache, Orchestrator, ViewId};
pub(crate) use location::{Repository, database_path};
pub(crate) use model::Backend;
#[cfg(test)]
pub(crate) use model::Workspace;
pub(crate) use model::{
    Activity, Checkpoint, HerdrSession, Item, ItemKey, Owner, Provider, PullRequest, Run, RunState,
    SourceKey,
};
pub(crate) use naming::LiveBranch;
pub(crate) use prompt::nonce;
pub(crate) use repo::github_repository;
pub(crate) use review::{CheckState, PrStatus, Review};
#[cfg(test)]
pub(crate) use service::SourceStatus;
pub(crate) use service::{Notice, Request, Service, Snapshot, SyncState, Timing};
pub(crate) use store::{Access, SCHEMA_VERSION, Store};
pub(crate) use view::{
    Event, List, LiveAgent, Look, OrchestratorView, RepoSpec, key_label, pane, without_html,
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
