//! Listing a repository's recent pull requests and issues, and making a pull
//! request's head branch reachable before a checkout is asked for. Every step
//! has a deadline and runs off the UI thread.

use super::{Accounts, Forge, Item, Kind, Origin, Target, gitlab, model::LIMIT, model::parse};
use crate::pull_request::{local_checkout, local_repository, remote_repository, run};
use std::time::{Duration, Instant};

pub(super) const TIMEOUT: Duration = Duration::from_secs(20);
/// Fetching one ref can wait on the network and on credential prompts that are
/// disabled for these children, so it gets its own, longer budget.
pub(super) const FETCH_TIMEOUT: Duration = Duration::from_secs(120);

// i18n: data
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

/// What a listing found: the forge, once the remote named it, and the
/// listing, which is `None` for a GitHub repository without a GitHub account.
pub(super) struct Listing {
    pub(super) forge: Option<Forge>,
    pub(super) listed: crate::Result<Option<(Origin, Vec<Item>)>>,
}

/// The repository's open pull requests and issues, most recently updated
/// first: a GitLab project's issues, else a GitHub repository's pull requests
/// and issues, which need a GitHub account. A GitLab listing that fails still
/// says the project is on GitLab, so its tab can open and say why.
pub(super) fn list(
    target: &Target,
    accounts: &Accounts,
    cancelled: impl Fn() -> bool,
    cooldown: &mut Option<Duration>,
) -> Listing {
    let deadline = Instant::now() + TIMEOUT;
    let project = match gitlab::project(target, deadline, &cancelled) {
        Ok(project) => project,
        Err(error) => {
            return Listing {
                forge: None,
                listed: Err(error),
            };
        }
    };
    if let Some(project) = project {
        return Listing {
            forge: Some(Forge::GitLab {
                host: project.source.host.clone(),
            }),
            listed: gitlab::list(
                target,
                &project,
                accounts.gitlab,
                deadline,
                &cancelled,
                cooldown,
            )
            .map(Some),
        };
    }
    let listed = match &accounts.github {
        Some(tokens) => github(target, tokens, deadline, cancelled, cooldown).map(Some),
        None => Ok(None),
    };
    Listing {
        forge: Some(Forge::GitHub),
        listed,
    }
}

/// A GitHub repository's open pull requests and issues.
fn github(
    target: &Target,
    tokens: &crate::github::Tokens,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    cooldown: &mut Option<Duration>,
) -> crate::Result<(Origin, Vec<Item>)> {
    let origin = Origin::from(match &target.host {
        crate::pull_request::Origin::Local => {
            local_repository(&target.input, deadline, &cancelled)?
        }
        crate::pull_request::Origin::Ssh(host) => {
            remote_repository(host, &target.input, deadline, &cancelled)?
        }
    });
    // The account that reads the listed repository's owner, which is the
    // upstream owner in a fork checkout.
    let token = tokens.for_owner(&origin.owner).clone();
    let timeout = deadline
        .checked_duration_since(Instant::now())
        .ok_or(sheprd_github::pull_request::Error::Timeout)?;
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
    target: &Target,
    item: &Item,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<()> {
    let deadline = Instant::now() + FETCH_TIMEOUT;
    if let crate::pull_request::Origin::Ssh(host) = &target.host {
        let directory = target
            .input
            .repo_key
            .as_deref()
            .ok_or(sheprd_github::pull_request::Error::Metadata)?;
        return super::remote::run(
            host,
            &super::remote::fetch_script(directory, item)?,
            deadline,
            cancelled,
        );
    }
    let checkout = local_checkout(&target.input, deadline, cancelled)?;
    let refspec = item.fetch_refspec();
    let mut command = sheprd_client::new_command("git");
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
    Err(sheprd_vcs::Error::from(crate::git::Error::Failed {
        operation: "fetch the pull request branch", // i18n: data
        details: crate::pull_request::clean(output.trim()),
    })
    .into())
}
