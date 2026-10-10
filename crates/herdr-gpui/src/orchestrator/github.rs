//! GitHub issues and pull requests, read with the GraphQL API and stored in
//! agent-launcher's row format: open issues, open pull requests, and the most
//! recently closed or merged ones. One query per page returns each pull
//! request's refs, SHAs, diff size and counts, which agent-launcher fetches
//! one pull request at a time.

use super::{Activity, Checkpoint, Error, Item, ItemKey, PullRequest, Result, SourceKey};
use chrono::{DateTime, Utc};
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

const PAGE: usize = 100;
/// The most open issues or open pull requests one sync reads. A repository
/// with more is shown but not cached, since a capped listing is no full sync.
pub(super) const MAX_OPEN: usize = 2_000;
/// Closed and merged pull requests kept, newest first.
const RECENTLY_CLOSED: usize = 50;
const TIMEOUT: Duration = Duration::from_secs(30);

const ISSUES: &str = r#"query($owner: String!, $repo: String!, $after: String) {
  repository(owner: $owner, name: $repo) {
    issues(first: 100, after: $after, states: OPEN, orderBy: {field: UPDATED_AT, direction: DESC}) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number title body state url createdAt updatedAt
        author { login }
        labels(first: 20) { nodes { name } }
        comments { totalCount }
      }
    }
  }
}"#;

const PULL_REQUESTS: &str = r#"query($owner: String!, $repo: String!, $after: String, $states: [PullRequestState!], $count: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequests(first: $count, after: $after, states: $states, orderBy: {field: UPDATED_AT, direction: DESC}) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number title body state isDraft url createdAt updatedAt
        author { login }
        labels(first: 20) { nodes { name } }
        comments { totalCount }
        commits { totalCount }
        additions deletions baseRefName headRefName baseRefOid headRefOid
        headRepository { nameWithOwner }
      }
    }
  }
}"#;

/// One sync's result. Only a `complete` listing may replace the cache.
pub(crate) struct Listing {
    pub(crate) items: Vec<Item>,
    pub(crate) checkpoint: Checkpoint,
    pub(crate) complete: bool,
}

/// Lists `source`. `previous` keeps agent-launcher's PR detail markers, which
/// stay valid because each embeds the pull request's revision.
pub(crate) fn sync(
    source: &SourceKey,
    token: &SecretString,
    previous: Option<&Checkpoint>,
    cancelled: &impl Fn() -> bool,
    cooldown: &mut Option<Duration>,
) -> Result<Listing> {
    let (owner, repo) = source
        .repository
        .split_once('/')
        .ok_or_else(|| Error::RemoteUrl(source.repository.clone()))?;
    let mut fetch = |query: &str, variables: Value| {
        crate::github::graphql(
            "orchestrator",
            token,
            query,
            variables,
            TIMEOUT,
            cancelled,
            cooldown,
        )
        .map_err(|error| match error {
            crate::Error::PrCancelled => Error::Cancelled,
            error => Error::Github(Box::new(error)),
        })
    };
    let mut complete = true;
    let mut items = Vec::new();

    let mut after = Value::Null;
    loop {
        let page: Page<IssueNode> = connection(
            fetch(
                ISSUES,
                json!({"owner": owner, "repo": repo, "after": after}),
            )?,
            "issues",
        )?;
        items.extend(page.nodes.into_iter().map(|node| node.into_item(source)));
        match next(&page.page_info, items.len()) {
            Next::Page(cursor) => after = cursor,
            Next::Done => break,
            Next::Capped => {
                complete = false;
                break;
            }
        }
    }

    let issues = items.len();
    let mut after = Value::Null;
    loop {
        let variables = json!({
            "owner": owner, "repo": repo, "after": after,
            "states": ["OPEN"], "count": PAGE,
        });
        let page: Page<PullRequestNode> =
            connection(fetch(PULL_REQUESTS, variables)?, "pullRequests")?;
        items.extend(page.nodes.into_iter().map(|node| node.into_item(source)));
        match next(&page.page_info, items.len() - issues) {
            Next::Page(cursor) => after = cursor,
            Next::Done => break,
            Next::Capped => {
                complete = false;
                break;
            }
        }
    }

    let variables = json!({
        "owner": owner, "repo": repo, "after": null,
        "states": ["CLOSED", "MERGED"], "count": RECENTLY_CLOSED,
    });
    let closed: Page<PullRequestNode> =
        connection(fetch(PULL_REQUESTS, variables)?, "pullRequests")?;
    items.extend(closed.nodes.into_iter().map(|node| node.into_item(source)));

    let previous = previous.cloned().unwrap_or_default();
    let checkpoint = Checkpoint {
        updated_at: items.iter().filter_map(|item| item.updated_at).max(),
        etag: None,
        last_full_at: Some(Utc::now()),
        pr_details: previous.pr_details,
        pr_cursor: previous.pr_cursor,
    };
    Ok(Listing {
        items,
        checkpoint,
        complete,
    })
}

enum Next {
    Page(Value),
    Done,
    Capped,
}

fn next(info: &PageInfo, read: usize) -> Next {
    match (&info.end_cursor, info.has_next_page) {
        (Some(_), true) if read >= MAX_OPEN => Next::Capped,
        (Some(cursor), true) => Next::Page(Value::String(cursor.clone())),
        _ => Next::Done,
    }
}

/// The `repository.<field>` connection of a GraphQL response.
pub(super) fn connection<T: serde::de::DeserializeOwned>(
    response: Value,
    field: &'static str,
) -> Result<Page<T>> {
    let value = response
        .get("data")
        .and_then(|data| data.get("repository"))
        .and_then(|repository| repository.get(field))
        .cloned()
        .ok_or(Error::Output {
            operation: "Reading GitHub",
        })?;
    serde_json::from_value(value).map_err(|source| Error::OutputJson {
        operation: "Reading GitHub",
        source,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Page<T> {
    page_info: PageInfo,
    pub(super) nodes: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

#[derive(Deserialize)]
struct Labels {
    nodes: Vec<Label>,
}

#[derive(Deserialize)]
struct Label {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Count {
    total_count: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct IssueNode {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    url: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    author: Option<Login>,
    labels: Option<Labels>,
    comments: Option<Count>,
}

impl IssueNode {
    pub(super) fn into_item(self, source: &SourceKey) -> Item {
        Item {
            key: ItemKey {
                source: source.clone(),
                native_id: self.number.to_string(),
            },
            identifier: format!("#{}", self.number),
            title: self.title,
            description: self.body.filter(|body| !body.is_empty()),
            // GraphQL says OPEN and CLOSED; the REST words agent-launcher stores
            // are lowercase.
            state: self.state.to_ascii_lowercase(),
            url: Some(self.url),
            author: self.author.map(|author| author.login),
            labels: self
                .labels
                .map(|labels| labels.nodes.into_iter().map(|label| label.name).collect())
                .unwrap_or_default(),
            parent_id: None,
            blocked_by: Vec::new(),
            priority: None,
            created_at: Some(self.created_at),
            updated_at: Some(self.updated_at),
            pull_request: None,
            activity: Some(Activity {
                comments: self.comments.map(|count| count.total_count),
                ..Activity::default()
            }),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PullRequestNode {
    #[serde(flatten)]
    issue: IssueNode,
    is_draft: bool,
    commits: Option<Count>,
    additions: Option<u64>,
    deletions: Option<u64>,
    base_ref_name: String,
    head_ref_name: String,
    base_ref_oid: String,
    head_ref_oid: String,
    head_repository: Option<NameWithOwner>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NameWithOwner {
    name_with_owner: String,
}

impl PullRequestNode {
    pub(super) fn into_item(self, source: &SourceKey) -> Item {
        let number = self.issue.number;
        let state = match self.issue.state.as_str() {
            "MERGED" => "merged",
            "CLOSED" => "closed",
            _ if self.is_draft => "draft",
            _ => "open",
        };
        let mut item = self.issue.into_item(source);
        item.key.native_id = format!("pr/{number}");
        item.state = state.to_owned();
        if let Some(activity) = item.activity.as_mut() {
            activity.commits = self.commits.map(|count| count.total_count);
        }
        item.pull_request = Some(PullRequest {
            number,
            additions: self.additions,
            deletions: self.deletions,
            base_ref: self.base_ref_name,
            head_ref: self.head_ref_name,
            base_sha: self.base_ref_oid,
            head_sha: self.head_ref_oid,
            head_repository: self.head_repository.map(|repo| repo.name_with_owner),
        });
        item
    }
}
