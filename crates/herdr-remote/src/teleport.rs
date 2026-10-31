//! Teleport: move a linked worktree to another connected host with its
//! branch, commits, uncommitted changes, tabs, running programs, and agent
//! sessions, then close the source workspace while keeping its checkout.
//!
//! The GUI connection's method allowlist lacks layouts, process details, and
//! agent sessions, so every step runs as a background script that calls Git
//! and the `herdr` CLI on the host concerned (locally, over SSH, or through a
//! cloud provider's authenticated command).

pub mod carry;
mod credentials;
pub mod error;
mod fresh;
mod git;
mod host;
pub mod job;
pub mod launch;
mod layout;
pub mod marks;
pub mod provision;
pub mod remote;
mod sessions;
pub mod snapshot;

pub use {
    error::{Error, Step},
    fresh::{
        Created, Naming, NewCheckout, Origin as FreshOrigin, Prepared, base_commit,
        dispatch_workspace, dispatch_worktree, prepare,
    },
    host::{Host, installed_programs_script},
    job::{HostRepositories, Place, Retired, Source, open_repositories},
    launch::AgentKind,
    marks::{Mark, Marks},
    snapshot::{
        Envelope, HostSnapshot, PaneInfoResult, ProcessInfo, ProcessInfoResult, SnapshotResult,
        WorkspaceCreated, WorktreeCreated,
    },
};
#[cfg(any(test, feature = "test-support"))]
pub use {
    git::SourceState,
    job::{Repository, plan::TabPlan},
    layout::Node,
    marks::Destination as MarkDestination,
};

/// The host Teleport, fan-out, and dispatch script for an endpoint, if they
/// can: each drives the endpoint's session through `herdr_cli`, which cannot
/// reach a development session.
pub fn host_for(target: &herdr_client::ConnectTarget) -> error::Result<Host> {
    let host = Host::new(target)?;
    if !host.scripts_herdr() {
        return Err(Error::DevelopmentSession);
    }
    Ok(host)
}

// Host scripts need /bin/sh; Teleport is not offered on other clients.
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod live_tests;
