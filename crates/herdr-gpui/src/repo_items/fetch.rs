//! Listing a repository's recent pull requests and issues, and making a pull
//! request's head branch reachable before a checkout is asked for. Every step
//! has a deadline and runs off the UI thread.

use super::{Item, Kind, Origin, model::LIMIT, model::parse};
use crate::{
    Error,
    pull_request::{Input, local_checkout, local_repository, run},
};
use std::{
    process::Command,
    time::{Duration, Instant},
};

pub(super) const TIMEOUT: Duration = Duration::from_secs(20);
/// Fetching one ref can wait on the network and on credential prompts that are
/// disabled for these children, so it gets its own, longer budget.
pub(super) const FETCH_TIMEOUT: Duration = Duration::from_secs(120);

const QUERY: &str = r#"query($owner: String!, $repo: String!, $count: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequests(first: $count, states: OPEN, orderBy: {field: UPDATED_AT, direction: DESC}) {
      nodes { number title isDraft headRefName author { login } headRepositoryOwner { login } }
    }
    issues(first: $count, states: OPEN, orderBy: {field: UPDATED_AT, direction: DESC}) {
      nodes { number title author { login } }
    }
  }
}"#;

/// The repository's open pull requests and issues, most recently updated first.
pub(super) fn list(
    input: &Input,
    token: &secrecy::SecretString,
    cancelled: impl Fn() -> bool,
    cooldown: &mut Option<Duration>,
) -> crate::Result<(Origin, Vec<Item>)> {
    let deadline = Instant::now() + TIMEOUT;
    let (owner, repo) = local_repository(input, deadline, &cancelled)?;
    let origin = Origin { owner, repo };
    let timeout = deadline
        .checked_duration_since(Instant::now())
        .ok_or(Error::PrTimeout)?;
    let response = crate::github::graphql(
        "repo_items",
        token,
        QUERY,
        serde_json::json!({"owner": origin.owner, "repo": origin.repo, "count": LIMIT}),
        timeout,
        cancelled,
        cooldown,
    )?;
    let mut items = parse(&response, &origin, Kind::PullRequest)?;
    items.extend(parse(&response, &origin, Kind::Issue)?);
    Ok((origin, items))
}

/// Update `origin/<head>` so the daemon can create a checkout on a pull request
/// branch this clone has not seen yet. A branch that already exists locally is
/// checked out by the daemon as-is, so a fetch failure is only fatal when the
/// remote ref is the only copy; the caller reports what Git said either way.
pub(super) fn fetch_branch(
    input: &Input,
    head: &str,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<()> {
    let deadline = Instant::now() + FETCH_TIMEOUT;
    let checkout = local_checkout(input, deadline, cancelled)?;
    let refspec = format!("+refs/heads/{head}:refs/remotes/origin/{head}");
    let mut command = Command::new("git");
    command
        .args(["-c", "core.fsmonitor=false", "-C", &checkout])
        .args(["fetch", "--no-tags", "--quiet", "origin", &refspec]);
    let (ok, output) = run(&mut command, deadline, cancelled)?;
    if ok {
        return Ok(());
    }
    Err(Error::GitFailed {
        operation: "fetch the pull request branch",
        details: crate::pull_request::clean(output.trim()),
    })
}
