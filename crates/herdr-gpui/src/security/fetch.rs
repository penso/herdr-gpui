//! Reading the alerts of every repository the window's workspaces hold, on a
//! background thread: each checkout's remote is read as the orchestrator
//! reads it, then per distinct github.com repository one GraphQL request for
//! its alerts and up to two REST requests for its private reports.

use super::{
    Error,
    model::{Account, Alerts, Checkout, Repo, parse, valid_name},
};
use crate::{github::Tokens, orchestrator::RepoSpec};
use herdr_client::ConnectTarget;
use secrecy::ExposeSecret;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(20);

/// Open alerts per repository. The section is a triage list, not a mirror:
/// GitHub's own page holds the rest, and the count says how many there are.
pub(super) const QUERY: &str = r#"query($owner: String!, $repo: String!) {
  repository(owner: $owner, name: $repo) {
    hasVulnerabilityAlertsEnabled
    vulnerabilityAlerts(first: 100, states: OPEN) {
      totalCount
      nodes {
        number createdAt vulnerableManifestPath
        securityVulnerability {
          severity vulnerableVersionRange
          firstPatchedVersion { identifier }
          package { name ecosystem }
          advisory { ghsaId summary }
        }
      }
    }
  }
}"#;

/// The alerts of each distinct github.com repository `specs` hold, in their
/// order. A checkout on another provider, or one whose remote cannot be
/// read, is left out; the orchestrator's own sections report those.
///
/// One repository open on several hosts is one card, but each host may sign
/// in to GitHub as someone else: when an account cannot read its alerts or
/// reports, the next one is tried, and the most readable answer is kept.
pub(super) fn read(specs: &[RepoSpec], cancelled: &AtomicBool) -> Vec<Repo> {
    let mut found: Vec<Found<'_>> = Vec::new();
    let mut checkouts: Vec<(String, Checkout)> = Vec::new();
    for spec in specs {
        if cancelled.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let Some(tokens) = &spec.request.token else {
            continue;
        };
        let name = match crate::orchestrator::github_repository(
            &spec.request.target,
            &spec.request.checkout,
            cancelled,
        ) {
            Ok(Some(name)) if valid_name(&name) => name,
            Ok(_) => continue,
            Err(error) => {
                tracing::debug!(category = "security", %error, "Cannot read a checkout's remote");
                continue;
            }
        };
        checkouts.push((
            name.clone(),
            Checkout {
                target: spec.request.target.clone(),
                workspace_id: spec.request.workspace_id.clone(),
                root: local_root(&spec.request.target, &spec.request.checkout),
            },
        ));
        let account = (spec.host.as_str(), tokens);
        match found
            .iter_mut()
            .find(|repo| repo.name.eq_ignore_ascii_case(&name))
        {
            Some(repo) => repo.add(account),
            None => found.push(Found {
                name,
                accounts: vec![account],
            }),
        }
    }
    let mut repos = Vec::new();
    // Once the quota is spent, the rest would only be refused too.
    let mut limited: Option<Arc<Error>> = None;
    for repo in found {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let mut best: Option<(u8, Repo)> = None;
        for (host, tokens) in repo.accounts {
            let alerts = match &limited {
                Some(error) => Err(error.clone()),
                None => alerts(&repo.name, tokens, cancelled).map_err(Arc::new),
            };
            note_limit(&alerts, &mut limited);
            let reports = match &limited {
                Some(error) => Err(error.clone()),
                None => {
                    super::reports::read(&repo.name, tokens, || cancelled.load(Ordering::Relaxed))
                        .map_err(Arc::new)
                }
            };
            note_limit(&reports, &mut limited);
            let read = Repo {
                name: repo.name.clone(),
                host: host.to_owned(),
                alerts,
                reports,
                account: Some(Account(tokens.clone())),
                checkouts: checkouts
                    .iter()
                    .filter(|(name, _)| name.eq_ignore_ascii_case(&repo.name))
                    .map(|(_, checkout)| checkout.clone())
                    .collect(),
            };
            let score = readable(&read);
            if best.as_ref().is_none_or(|(kept, _)| score > *kept) {
                best = Some((score, read));
            }
            if score == FULLY_READ || limited.is_some() || cancelled.load(Ordering::Relaxed) {
                break;
            }
        }
        repos.extend(best.map(|(_, read)| read));
    }
    repos
}

/// The repository root holding `checkout` on this machine: the nearest
/// folder with a `.git` entry. Another host's files are out of reach.
fn local_root(target: &ConnectTarget, checkout: &str) -> Option<PathBuf> {
    if target.is_remote() {
        return None;
    }
    Path::new(checkout)
        .ancestors()
        .find(|folder| folder.join(".git").exists())
        .map(Path::to_path_buf)
}

/// A repository and the accounts its hosts read GitHub with, each with the
/// host it came from.
pub(super) struct Found<'a> {
    pub(super) name: String,
    pub(super) accounts: Vec<(&'a str, &'a Tokens)>,
}

impl<'a> Found<'a> {
    /// Adds another host's account, unless it reads this repository with a
    /// token already listed: asking again would only repeat the answer.
    pub(super) fn add(&mut self, account: (&'a str, &'a Tokens)) {
        let Some((owner, _)) = self.name.split_once('/') else {
            return;
        };
        let token = account.1.for_owner(owner).expose_secret();
        if self
            .accounts
            .iter()
            .all(|(_, tokens)| tokens.for_owner(owner).expose_secret() != token)
        {
            self.accounts.push(account);
        }
    }
}

/// A read whose alerts and reports both came back.
pub(super) const FULLY_READ: u8 = 2;

/// How much of a repository one account could read: its alerts listing,
/// and its private reporting, whether on or off.
pub(super) fn readable(repo: &Repo) -> u8 {
    u8::from(matches!(repo.alerts, Ok(Alerts::Open { .. } | Alerts::Off)))
        + u8::from(matches!(
            repo.reports,
            Ok(super::Reports::Open { .. } | super::Reports::Off)
        ))
}

/// Remembers a spent quota, so the round's remaining reads are skipped.
fn note_limit<T>(read: &Result<T, Arc<Error>>, limited: &mut Option<Arc<Error>>) {
    if let Err(error) = read
        && error.rate_limited()
    {
        *limited = Some(error.clone());
    }
}

fn alerts(name: &str, tokens: &Tokens, cancelled: &AtomicBool) -> Result<Alerts, Error> {
    let (owner, repo) = name.split_once('/').ok_or(Error::Output)?;
    // Each repository's refusal is its own: a token that may not read one
    // repository's alerts must not pause the others, as the shared cooldown
    // would. An exhausted quota stops the round through `limited` instead.
    let mut cooldown = None;
    let response = crate::github::graphql(
        "security",
        tokens.for_owner(owner),
        QUERY,
        serde_json::json!({"owner": owner, "repo": repo}),
        TIMEOUT,
        || cancelled.load(Ordering::Relaxed),
        &mut cooldown,
    )
    .map_err(|error| Error::Github(Box::new(error)))?;
    let alerts = parse(&response)?;
    if !matches!(alerts, Alerts::Open { total: 0, .. }) {
        return Ok(alerts);
    }
    let check = crate::github::rest::<serde_json::Value>(
        "security",
        tokens.for_owner(owner),
        &format!("/repos/{name}/dependabot/alerts?state=open&per_page=1"),
        TIMEOUT,
        || cancelled.load(Ordering::Relaxed),
    );
    Ok(checked(alerts, check.map(drop)))
}

/// An empty listing as REST's `check` of it says: whatever else goes wrong
/// there, it stands, and only a refusal says it was not the whole story.
pub(super) fn checked(empty: Alerts, check: crate::Result<()>) -> Alerts {
    match check {
        Err(error) if super::reports::hidden(&error) => Alerts::Hidden,
        _ => empty,
    }
}
