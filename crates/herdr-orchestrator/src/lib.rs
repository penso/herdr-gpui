//! The orchestrator's backend: a repository's issues and pull requests, and
//! the agent runs dispatched for them, without GPUI.
//!
//! Its records live in its own per-repository SQLite database (`location`),
//! whose schema began as agent-launcher's; agent-launcher's file is copied
//! in the first time a repository opens. A restart shows what the database
//! and the app's cache (`cache`) hold before reading anything again;
//! `docs/orchestrator.md` has the details. The worker (`service`) blocks on
//! the network, Git, and the disk, so it runs on its own thread; the app
//! owns the views and the window's handle on each worker.

#![cfg_attr(test, allow(clippy::disallowed_methods))]

pub mod actions;
pub mod beads;
pub mod cache;
mod error;
pub mod git;
pub mod github;
pub mod gitlab;
pub mod location;
pub mod model;
pub mod naming;
pub mod private_file;
pub mod prompt;
pub mod repo;
pub mod review;
pub mod service;
mod ssh_slots;
pub mod store;

pub use actions::{Action, DispatchRequest};
pub use error::{Error, Result};
pub use location::{Repository, database_path};
#[cfg(any(test, feature = "test-support"))]
pub use model::Workspace;
pub use model::{
    Activity, Backend, Checkpoint, HerdrSession, Item, ItemKey, Owner, Provider, PullRequest, Run,
    RunState, SourceKey,
};
pub use naming::LiveBranch;
pub use prompt::nonce;
pub use repo::github_repository;
pub use review::{CheckState, PrStatus, Review};
#[cfg(any(test, feature = "test-support"))]
pub use service::SourceStatus;
pub use service::{Notice, Request, Service, Snapshot, SyncState, Timing};
pub use store::{Access, SCHEMA_VERSION, Store};

#[cfg(all(test, unix))]
#[path = "../../test-support/executable.rs"]
mod test_executable;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
