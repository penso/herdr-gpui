//! The orchestrator as the window shows it: the Pull requests and Issues
//! sections, an item's pages, and the window's handle on each repository's
//! worker. `herdr-orchestrator` owns the records, the database, the readers,
//! and the worker; `docs/orchestrator.md` has the details.

mod host;
mod section;
mod view;

pub(crate) use herdr_orchestrator::*;
pub(crate) use host::{LiveCache, Orchestrator, ViewId};
pub(crate) use view::{
    Event, GitLabRead, List, LiveAgent, Look, OrchestratorView, RepoSpec, key_label, pane,
    without_html,
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
