#![allow(clippy::unwrap_used)]

use super::{
    Base, Cache, Input, Lookup, Origin, Remote, Result, base_repository, clean,
    fetch::{
        Ask, OUTPUT_LIMIT, TIMEOUT, fetch, local_repository, origin_repository, remote_host,
        target, worktree_checkout,
    },
    fixture,
    lookup::Worker,
    model::{MergeState, Outcome, State},
    parse::{parse, parse_graphql},
    run,
};
use crate::Error;
use std::{
    process::Command,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

mod actions;
mod batch;
mod by_number;
mod cache;
mod fetch;
mod fork_branch;
mod fork_remote;
mod lookup;
mod moved_checkout;
mod responses;
mod reused_branch;
mod routing;
mod upstream;
mod workspace_directory;

fn response() -> serde_json::Value {
    serde_json::json!([{
        "number":8, "url":"https://github.com/example/project/pull/8", "title":"Title\n\u{202e}safe",
        "state":"OPEN", "isDraft":false, "headRefName":"feature", "baseRefName":"main",
        "additions":1, "deletions":0, "changedFiles":1, "updatedAt":"now",
        "mergeStateStatus":"UNKNOWN", "reviewDecision":"", "headRepositoryOwner":{"login":"example"}
    }])
}

/// What a checkout with only a GitHub `origin` resolves to.
fn origin_base(owner: &str, repo: &str) -> Base {
    Base {
        owner: owner.into(),
        repo: repo.into(),
        remote: Remote::Origin,
        fork_owner: None,
    }
}
