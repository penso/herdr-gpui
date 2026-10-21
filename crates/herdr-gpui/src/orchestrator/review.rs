//! Where each open pull request stands on GitHub: its review decision, who
//! is asked to review it, and its checks, read with one GraphQL query per
//! page after each sync. The last answer is kept in the database beside the
//! items, so a restart shows it until the first sync reads it again.

use super::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, time::Duration};

const QUERY: &str = r#"query($owner: String!, $repo: String!, $after: String) {
  repository(owner: $owner, name: $repo) {
    pullRequests(first: 50, after: $after, states: OPEN, orderBy: {field: UPDATED_AT, direction: DESC}) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number
        reviewDecision
        reviewRequests(first: 20) {
          nodes { requestedReviewer { __typename ... on User { login } ... on Team { slug } } }
        }
        commits(last: 1) {
          nodes { commit { statusCheckRollup { contexts(first: 50) {
            nodes {
              __typename
              ... on CheckRun { name status conclusion }
              ... on StatusContext { context state }
            }
          } } } }
        }
      }
    }
  }
}"#;

/// Pages of fifty read per sync: the most recently updated two hundred open
/// pull requests carry a status, older ones show none.
const PAGES: usize = 4;
/// Checks kept per pull request, and reviewers asked.
const MAX_CHECKS: usize = 50;
const MAX_REVIEWERS: usize = 20;
/// Characters kept of a check's or reviewer's name: display text.
const MAX_NAME: usize = 100;
const TIMEOUT: Duration = Duration::from_secs(30);

/// GitHub's verdict on a pull request's reviews.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Review {
    Approved,
    ChangesRequested,
    Required,
}

/// Where one check stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum CheckState {
    Failed,
    Pending,
    Passed,
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Check {
    pub(crate) name: String,
    pub(crate) state: CheckState,
}

/// One open pull request's review and checks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PrStatus {
    pub(crate) review: Option<Review>,
    /// Logins of the users asked to review, and `team/slug` for teams.
    pub(crate) requested: Vec<String>,
    pub(crate) checks: Vec<Check>,
}

impl PrStatus {
    /// What the checks add up to: any failure, else anything still running,
    /// else passed. None when nothing but skipped checks reported.
    pub(crate) fn checks_state(&self) -> Option<CheckState> {
        [CheckState::Failed, CheckState::Pending, CheckState::Passed]
            .into_iter()
            .find(|wanted| self.checks.iter().any(|check| check.state == *wanted))
    }

    /// Whether `login` is asked to review it.
    pub(crate) fn requests(&self, login: &str) -> bool {
        self.requested
            .iter()
            .any(|asked| asked.eq_ignore_ascii_case(login))
    }
}

/// Every open pull request's status in `owner/repo`, by number.
pub(super) fn fetch(
    repository: &str,
    mut fetch: impl FnMut(&str, Value) -> Result<Value>,
) -> Result<HashMap<u64, PrStatus>> {
    let (owner, repo) = repository
        .split_once('/')
        .ok_or_else(|| Error::RemoteUrl(repository.to_owned()))?;
    let mut statuses = HashMap::new();
    let mut after: Option<String> = None;
    for _ in 0..PAGES {
        let response = fetch(
            QUERY,
            json!({ "owner": owner, "repo": repo, "after": after }),
        )?;
        let page = parse(response)?;
        statuses.extend(page.statuses);
        match page.next {
            Some(cursor) => after = Some(cursor),
            None => break,
        }
    }
    Ok(statuses)
}

/// The GraphQL timeout a status read allows.
pub(super) fn timeout() -> Duration {
    TIMEOUT
}

pub(super) struct Page {
    pub(super) statuses: HashMap<u64, PrStatus>,
    /// The cursor of the next page, when there is one.
    pub(super) next: Option<String>,
}

pub(super) fn parse(response: Value) -> Result<Page> {
    let value = response
        .get("data")
        .and_then(|data| data.get("repository"))
        .and_then(|repository| repository.get("pullRequests"))
        .cloned()
        .ok_or(Error::Output {
            operation: "Reading pull request checks",
        })?;
    let connection: Connection =
        serde_json::from_value(value).map_err(|source| Error::OutputJson {
            operation: "Reading pull request checks",
            source,
        })?;
    let statuses = connection
        .nodes
        .into_iter()
        .map(|node| (node.number, node.into_status()))
        .collect();
    let next = connection
        .page_info
        .has_next_page
        .then_some(connection.page_info.end_cursor)
        .flatten();
    Ok(Page { statuses, next })
}

fn name(text: &str) -> String {
    crate::notifications::safe_text(text, MAX_NAME)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection {
    page_info: PageInfo,
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Node {
    number: u64,
    review_decision: Option<String>,
    review_requests: Option<Nodes<Request>>,
    commits: Option<Nodes<CommitNode>>,
}

#[derive(Deserialize)]
struct Nodes<T> {
    nodes: Vec<Option<T>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    requested_reviewer: Option<Reviewer>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum Reviewer {
    User {
        login: String,
    },
    Team {
        slug: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct CommitNode {
    commit: Commit,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commit {
    status_check_rollup: Option<Rollup>,
}

#[derive(Deserialize)]
struct Rollup {
    contexts: Nodes<Entry>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum Entry {
    CheckRun {
        name: String,
        status: Option<String>,
        conclusion: Option<String>,
    },
    StatusContext {
        context: String,
        state: Option<String>,
    },
    #[serde(other)]
    Other,
}

impl Node {
    fn into_status(self) -> PrStatus {
        let review = match self.review_decision.as_deref() {
            Some("APPROVED") => Some(Review::Approved),
            Some("CHANGES_REQUESTED") => Some(Review::ChangesRequested),
            Some("REVIEW_REQUIRED") => Some(Review::Required),
            _ => None,
        };
        let requested = self
            .review_requests
            .map(|requests| requests.nodes)
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .filter_map(|request| match request.requested_reviewer? {
                Reviewer::User { login } => Some(name(&login)),
                Reviewer::Team { slug } => Some(format!("team/{}", name(&slug))),
                Reviewer::Other => None,
            })
            .take(MAX_REVIEWERS)
            .collect();
        let checks = self
            .commits
            .map(|commits| commits.nodes)
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .filter_map(|node| node.commit.status_check_rollup)
            .flat_map(|rollup| rollup.contexts.nodes)
            .flatten()
            .filter_map(|context| match context {
                Entry::CheckRun {
                    name: check,
                    status,
                    conclusion,
                } => Some(Check {
                    name: name(&check),
                    state: check_run(status.as_deref(), conclusion.as_deref()),
                }),
                Entry::StatusContext { context, state } => Some(Check {
                    name: name(&context),
                    state: status_context(state.as_deref()),
                }),
                Entry::Other => None,
            })
            .take(MAX_CHECKS)
            .collect();
        PrStatus {
            review,
            requested,
            checks,
        }
    }
}

fn check_run(status: Option<&str>, conclusion: Option<&str>) -> CheckState {
    if status != Some("COMPLETED") {
        return CheckState::Pending;
    }
    match conclusion {
        Some("SUCCESS") => CheckState::Passed,
        Some("NEUTRAL" | "SKIPPED") => CheckState::Skipped,
        _ => CheckState::Failed,
    }
}

fn status_context(state: Option<&str>) -> CheckState {
    match state {
        Some("SUCCESS") => CheckState::Passed,
        Some("PENDING" | "EXPECTED") | None => CheckState::Pending,
        _ => CheckState::Failed,
    }
}

#[cfg(test)]
mod tests;
