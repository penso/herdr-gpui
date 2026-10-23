//! Read-only PR prefetch. One worker, bounded memory, and no UI-thread I/O.
//! Writes to a pull request live in `pr_actions`, behind explicit user actions.

mod batch;
mod cache;
mod error;
mod fetch;
mod lookup;
mod model;
mod parse;
mod remote;
mod store;

#[cfg(test)]
mod tests;

pub(crate) use error::Error;
pub(crate) use {
    cache::Cache,
    fetch::{local_checkout, run, run_bytes},
    lookup::Lookup,
    model::{
        Input, MergeMethod, Origin, Outcome, PullRequest, ReviewDecision, State, clean,
        repository_input, workspace_input,
    },
    remote::{Base, Remote, base_repository},
    store::{Shared, View},
};

pub(crate) use fetch::{by_number, local_repository, repository_key};
pub(crate) use model::MergeState;
#[cfg(any(test, all(feature = "integration-test", target_os = "macos")))]
pub(crate) use model::fixture;

use fetch::Origins;
use parse::{parse_graphql, parse_numbered};

/// Absence is a successful lookup with no PR, not an error.
type Result = crate::Result<Option<PullRequest>>;
