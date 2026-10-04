#![allow(clippy::unwrap_used)]

use super::{
    Cache, Input, Lookup, Origin, Result,
    cache::{CACHE_LIMIT, ERROR_BACKOFF, REFRESH},
    clean,
    fetch::{OUTPUT_LIMIT, TIMEOUT, fetch, local_repository, remote_host, worktree_checkout},
    fixture,
    lookup::Worker,
    model::{MergeState, State},
    parse::{parse, parse_graphql},
    run,
};
use crate::Error;
use std::{
    process::Command,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

mod cache;
mod responses;
mod upstream;
mod worker;

fn input(branch: &str) -> Input {
    Input {
        checkout: None,
        repo_key: "/repo/.git".into(),
        branch: branch.into(),
    }
}

fn response() -> serde_json::Value {
    serde_json::json!([{
        "number":8, "url":"https://github.com/example/project/pull/8", "title":"Title\n\u{202e}safe",
        "state":"OPEN", "isDraft":false, "headRefName":"feature", "baseRefName":"main",
        "additions":1, "deletions":0, "changedFiles":1, "updatedAt":"now",
        "mergeStateStatus":"UNKNOWN", "reviewDecision":"", "headRepositoryOwner":{"login":"example"}
    }])
}
