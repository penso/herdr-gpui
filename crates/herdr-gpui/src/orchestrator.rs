//! The orchestrator: a repository's issues and pull requests, and the agent
//! runs dispatched for them.
//!
//! Its records live in the per-repository SQLite database agent-launcher also
//! uses, so both applications can run at the same time against one
//! repository, the way herdr and herdr-gpui share a daemon. agent-launcher's
//! `docs/shared-state.md` is the contract this module implements: the file's
//! location, its schema version, key formats, and which application may write
//! which rows. herdr-gpui writes only the runs it owns, and never takes
//! agent-launcher's `runtime.lock`.

mod actions;
mod beads;
mod error;
mod github;
mod location;
mod model;
mod prompt;
mod repo;
mod service;
mod store;
mod tab;
mod view;
mod window;

pub(crate) use actions::{Action, DispatchRequest};
pub(crate) use error::{Error, Result};
pub(crate) use location::{Repository, database_path};
pub(crate) use model::{
    Activity, Checkpoint, HerdrSession, Item, ItemKey, Owner, Provider, PullRequest, Run, RunState,
    SourceKey,
};
#[cfg(test)]
pub(crate) use model::{Backend, Workspace};
#[cfg(test)]
pub(crate) use service::SourceStatus;
pub(crate) use service::{Notice, Request, Service, Snapshot, SyncState, Timing};
pub(crate) use store::{Access, SCHEMA_VERSION, Store};
pub(crate) use tab::{LiveCache, Orchestrator};
pub(crate) use view::{Event, LiveAgent, Look, OrchestratorView};
pub(crate) use window::Detached;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
