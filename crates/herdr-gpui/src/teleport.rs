//! Teleport: move a linked worktree to another connected host with its
//! branch, commits, uncommitted changes, tabs, running programs, and agent
//! sessions, then close the source workspace while keeping its checkout.
//!
//! The GUI connection's method allowlist lacks layouts, process details, and
//! agent sessions, so every step runs as a background script that calls Git
//! and the `herdr` CLI on the host concerned (locally or over SSH).

mod credentials;
mod error;
mod git;
mod host;
mod job;
mod launch;
mod layout;
mod marks;
mod provision;
mod remote;
mod sessions;
mod snapshot;
mod ui;

#[cfg(test)]
pub(crate) use marks::Destination as MarkDestination;
pub(crate) use {
    host::Host,
    job::{HostRepositories, Place, Repository, Retired, Source},
    marks::{Mark, Marks},
    ui::{Follow, Teleport},
};

/// The host Teleport scripts for an endpoint, if it can script it at all.
pub(crate) fn host_for(target: &herdr_client::ConnectTarget) -> error::Result<Host> {
    Host::new(target)
}

// Host scripts need /bin/sh; Teleport is not offered on other clients.
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod live_tests;
