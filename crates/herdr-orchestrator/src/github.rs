//! GitHub issues and pull requests, read with the GraphQL API and stored in
//! agent-launcher's row format: open issues, open pull requests, and the most
//! recently closed or merged ones. One query per page returns each pull
//! request's refs, SHAs, diff size and counts, which agent-launcher fetches
//! one pull request at a time.
//!
//! Between full listings a sync reads only what changed since the last one:
//! issues GitHub filters by update time, and pull requests newest first until
//! one is older. A quiet repository then costs two small requests a minute
//! however many items it has, instead of a page per hundred.

use super::{Activity, Checkpoint, Error, Item, ItemKey, PullRequest, Result, SourceKey};
use chrono::{DateTime, TimeDelta, Utc};
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::time::Duration;

const PAGE: usize = 100;
/// The most open issues or open pull requests one sync reads. A repository
/// with more is shown but not cached, since a capped listing is no full sync.
pub const MAX_OPEN: usize = 2_000;
/// Closed and merged pull requests kept, newest first.
const RECENTLY_CLOSED: usize = 50;
const TIMEOUT: Duration = Duration::from_secs(30);
/// How long incremental syncs follow one another before a full listing again
/// catches what they cannot see, such as a deleted or transferred issue.
pub const FULL_EVERY: TimeDelta = TimeDelta::minutes(30);
/// Pull requests read per page when only changes are wanted: usually the
/// first page already reaches one older than the last sync.
const CHANGED_PAGE: usize = 20;

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

/// Open and closed issues updated since the last sync: a closed one leaves
/// the listing, as a full sync of open issues leaves it out.
const CHANGED_ISSUES: &str = r#"query($owner: String!, $repo: String!, $after: String, $since: DateTime!) {
  repository(owner: $owner, name: $repo) {
    issues(first: 100, after: $after, filterBy: {since: $since, states: [OPEN, CLOSED]}, orderBy: {field: UPDATED_AT, direction: DESC}) {
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

/// How far a full listing's start may be from GitHub's clock. A full listing
/// resumes from just before it began, so what changes while it reads is
/// read again; a local clock this far ahead could skip a change until the
/// next full listing.
const CLOCK_MARGIN: TimeDelta = TimeDelta::minutes(5);

/// Where each listing of the next incremental sync continues from. Issues
/// and pull requests are read one after the other, so each keeps its own:
/// an issue that changes after its listing was read may predate a pull
/// request the later listing saw, and a quiet listing must not hold back an
/// active one. The sync worker keeps it between syncs; the checkpoint
/// stores the older of the two, so a restart resumes safely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resume {
    pub issues: DateTime<Utc>,
    pub pulls: DateTime<Utc>,
}

impl Resume {
    fn both(at: DateTime<Utc>) -> Self {
        Self {
            issues: at,
            pulls: at,
        }
    }

    pub fn oldest(self) -> DateTime<Utc> {
        self.issues.min(self.pulls)
    }
}

/// One sync's result. Only a `complete` listing may replace the cache.
pub struct Listing {
    pub items: Vec<Item>,
    pub checkpoint: Checkpoint,
    pub complete: bool,
    pub resume: Resume,
}

/// Lists `source`. `previous` keeps agent-launcher's PR detail markers, which
/// stay valid because each embeds the pull request's revision. With `cached`,
/// the items the last complete sync stored, only what changed since is read
/// while the last full listing is recent, from `resume` (else the checkpoint).
pub fn sync(
    source: &SourceKey,
    token: &SecretString,
    previous: Option<&Checkpoint>,
    (cached, resume): (Option<Vec<Item>>, Option<Resume>),
    cancelled: &impl Fn() -> bool,
    cooldown: &mut Option<Duration>,
) -> Result<Listing> {
    let (owner, repo) = source
        .repository
        .split_once('/')
        .ok_or_else(|| Error::RemoteUrl(source.repository.clone()))?;
    let mut fetch = |query: &str, variables: Value| {
        herdr_github::graphql(
            "orchestrator",
            token,
            query,
            variables,
            TIMEOUT,
            cancelled,
            cooldown,
        )
        .map_err(|error| match error {
            herdr_github::Error::PullRequest(herdr_github::pull_request::Error::Cancelled) => {
                Error::Cancelled
            }
            error => Error::Github(Box::new(error)),
        })
    };
    let previous = previous.cloned().unwrap_or_default();
    if let Some((since, cached)) = incremental(&previous, cached, resume, Utc::now())
        && let Some(listing) = changes(source, (owner, repo), since, cached, &previous, &mut fetch)?
    {
        return Ok(listing);
    }
    full(source, (owner, repo), previous, &mut fetch)
}

/// Where an incremental sync continues from, while the last full listing is
/// recent enough and its items are at hand.
pub fn incremental(
    previous: &Checkpoint,
    cached: Option<Vec<Item>>,
    resume: Option<Resume>,
    now: DateTime<Utc>,
) -> Option<(Resume, Vec<Item>)> {
    let since = resume.or_else(|| previous.updated_at.map(Resume::both))?;
    let full = previous.last_full_at?;
    (now - full < FULL_EVERY).then_some(())?;
    Some((since, cached?))
}

pub fn full(
    source: &SourceKey,
    (owner, repo): (&str, &str),
    previous: Checkpoint,
    fetch: &mut impl FnMut(&str, Value) -> Result<Value>,
) -> Result<Listing> {
    let started = Utc::now() - CLOCK_MARGIN;
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

    // Not from the newest change seen: the open and closed pull requests are
    // separate reads, so one opened after the first may predate one closed
    // before the second, and would be skipped until the next full listing.
    let resume = Resume::both(started);
    let checkpoint = Checkpoint {
        updated_at: Some(started),
        etag: None,
        last_full_at: Some(Utc::now()),
        pr_details: previous.pr_details,
        pr_cursor: previous.pr_cursor,
    };
    Ok(Listing {
        items,
        checkpoint,
        complete,
        resume,
    })
}

/// The cached listing with what changed since `since` applied, or `None`
/// when so much changed that a full listing is the cheaper read.
fn changes(
    source: &SourceKey,
    (owner, repo): (&str, &str),
    since: Resume,
    cached: Vec<Item>,
    previous: &Checkpoint,
    fetch: &mut impl FnMut(&str, Value) -> Result<Value>,
) -> Result<Option<Listing>> {
    let mut changed = Vec::new();
    let mut after = Value::Null;
    loop {
        let variables = json!({
            "owner": owner, "repo": repo, "after": after, "since": since.issues.to_rfc3339(),
        });
        let page: Page<IssueNode> = connection(fetch(CHANGED_ISSUES, variables)?, "issues")?;
        changed.extend(page.nodes.into_iter().map(|node| node.into_item(source)));
        match next(&page.page_info, changed.len()) {
            Next::Page(cursor) => after = cursor,
            Next::Done => break,
            Next::Capped => return Ok(None),
        }
    }
    let issues = changed.len();
    let mut after = Value::Null;
    'pages: loop {
        let variables = json!({
            "owner": owner, "repo": repo, "after": after,
            "states": ["OPEN", "CLOSED", "MERGED"], "count": CHANGED_PAGE,
        });
        let page: Page<PullRequestNode> =
            connection(fetch(PULL_REQUESTS, variables)?, "pullRequests")?;
        for node in page.nodes {
            let item = node.into_item(source);
            if item.updated_at.is_some_and(|at| at < since.pulls) {
                break 'pages;
            }
            changed.push(item);
        }
        match next(&page.page_info, changed.len() - issues) {
            Next::Page(cursor) => after = cursor,
            Next::Done => break,
            Next::Capped => return Ok(None),
        }
    }
    let resume = Resume {
        issues: advance(since.issues, &changed[..issues]),
        pulls: advance(since.pulls, &changed[issues..]),
    };
    let items = merge(cached, changed);
    let open = |pull_request: bool| {
        items
            .iter()
            .filter(|item| item.pull_request.is_some() == pull_request && !settled(item))
            .count()
    };
    let complete = open(false) <= MAX_OPEN && open(true) <= MAX_OPEN;
    let checkpoint = Checkpoint {
        updated_at: Some(resume.oldest()),
        etag: None,
        last_full_at: previous.last_full_at,
        pr_details: previous.pr_details.clone(),
        pr_cursor: previous.pr_cursor,
    };
    Ok(Some(Listing {
        items,
        checkpoint,
        complete,
        resume,
    }))
}

/// Where one listing continues: the newest change it saw, never before
/// `since`. Anything that changes after the listing was read is newer still,
/// and `since` is inclusive, so nothing is skipped.
pub fn advance(since: DateTime<Utc>, listed: &[Item]) -> DateTime<Utc> {
    listed
        .iter()
        .filter_map(|item| item.updated_at)
        .fold(since, DateTime::max)
}

/// A closed or merged pull request.
fn settled(item: &Item) -> bool {
    item.pull_request.is_some() && matches!(item.state.as_str(), "closed" | "merged")
}

/// Applies `changed`, read newest first, to the cached items: each replaces
/// its older copy, a closed issue is dropped, and only the newest
/// [`RECENTLY_CLOSED`] closed or merged pull requests are kept, as a full
/// listing would hold them.
pub fn merge(mut items: Vec<Item>, changed: Vec<Item>) -> Vec<Item> {
    let mut seen = HashSet::new();
    let changed: Vec<Item> = changed
        .into_iter()
        .filter(|item| seen.insert(item.key.clone()))
        .collect();
    items.retain(|item| !seen.contains(&item.key));
    items.extend(
        changed
            .into_iter()
            .filter(|item| item.pull_request.is_some() || item.state != "closed"),
    );
    let mut closed: Vec<_> = items
        .iter()
        .filter(|item| settled(item))
        .map(|item| (item.updated_at, item.key.clone()))
        .collect();
    closed.sort_by_key(|(updated, _)| std::cmp::Reverse(*updated));
    let dropped: HashSet<ItemKey> = closed
        .into_iter()
        .skip(RECENTLY_CLOSED)
        .map(|(_, key)| key)
        .collect();
    items.retain(|item| !dropped.contains(&item.key));
    items
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
pub fn connection<T: serde::de::DeserializeOwned>(
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
pub struct Page<T> {
    page_info: PageInfo,
    pub nodes: Vec<T>,
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
pub struct IssueNode {
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
    pub fn into_item(self, source: &SourceKey) -> Item {
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
pub struct PullRequestNode {
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
    pub fn into_item(self, source: &SourceKey) -> Item {
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
