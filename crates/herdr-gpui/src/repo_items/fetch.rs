//! Listing a repository's recent pull requests and issues, and making a pull
//! request's head branch reachable before a checkout is asked for. Every step
//! has a deadline and runs off the UI thread.

use super::{Item, Kind, Origin, model::LIMIT, model::parse};
use crate::pull_request::{Input, local_checkout, local_repository, run};
use std::time::{Duration, Instant};

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
    tokens: &crate::github::Tokens,
    cancelled: impl Fn() -> bool,
    cooldown: &mut Option<Duration>,
) -> crate::Result<(Origin, Vec<Item>)> {
    let deadline = Instant::now() + TIMEOUT;
    let origin = Origin::from(local_repository(input, deadline, &cancelled)?);
    // The account that reads the listed repository's owner, which is the
    // upstream owner in a fork checkout.
    let token = tokens.for_owner(&origin.owner).clone();
    let timeout = deadline
        .checked_duration_since(Instant::now())
        .ok_or(herdr_github::pull_request::Error::Timeout)?;
    let response = crate::github::graphql(
        "repo_items",
        &token,
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

/// Fetch the PR's head from the remote it lives on, including GitHub's
/// published fork PR refs on the listed repository's remote.
/// Existing local branches are never moved by this fetch.
pub(super) fn fetch_branch(
    input: &Input,
    item: &Item,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<()> {
    let deadline = Instant::now() + FETCH_TIMEOUT;
    let checkout = local_checkout(input, deadline, cancelled)?;
    let refspec = item.fetch_refspec();
    let mut command = herdr_client::new_command("git");
    command
        .args(["-c", "core.fsmonitor=false", "-C", &checkout])
        .args([
            "fetch",
            "--no-tags",
            "--quiet",
            item.remote.name(),
            &refspec,
        ]);
    let (ok, output) = run(&mut command, deadline, cancelled)?;
    if ok {
        return Ok(());
    }
    Err(herdr_vcs::Error::from(crate::git::Error::Failed {
        operation: "fetch the pull request branch",
        details: crate::pull_request::clean(output.trim()),
    })
    .into())
}
