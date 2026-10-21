//! What the sections list, derived from a snapshot without drawing: issues
//! with Beads children under their parents, grouped by priority, and pull
//! requests grouped by what each waits on. Pure, so it is tested without a
//! window.

use crate::orchestrator::{
    Backend, CheckState, HerdrSession, Item, PrStatus, Provider, Review, Run, RunState,
};
use herdr_client::protocol::AgentStatus;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tab {
    #[default]
    Issues,
    PullRequests,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Sort {
    #[default]
    Updated,
    Newest,
    Oldest,
    Priority,
    Title,
}

impl Sort {
    pub(crate) const ALL: [Self; 5] = [
        Self::Updated,
        Self::Newest,
        Self::Oldest,
        Self::Priority,
        Self::Title,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Updated => "Recently updated",
            Self::Newest => "Newest",
            Self::Oldest => "Oldest",
            Self::Priority => "Priority",
            Self::Title => "Title",
        }
    }

    pub(crate) fn compare(self, left: &Item, right: &Item) -> Ordering {
        let tie = || left.key.canonical().cmp(&right.key.canonical());
        match self {
            Self::Updated => right.updated_at.cmp(&left.updated_at),
            Self::Newest => right.created_at.cmp(&left.created_at),
            Self::Oldest => left.created_at.cmp(&right.created_at),
            // Beads' P0 is the most urgent; items without one go last.
            Self::Priority => match (left.priority, right.priority) {
                (Some(left), Some(right)) => left.cmp(&right),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            Self::Title => left.title.to_lowercase().cmp(&right.title.to_lowercase()),
        }
        .then_with(tie)
    }
}

/// Narrowing beyond the search text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Filters {
    /// Only one provider's items.
    pub(crate) provider: Option<Provider>,
    pub(crate) has_run: bool,
    pub(crate) blocked: bool,
    /// Pull requests: only open and draft ones.
    pub(crate) open_only: bool,
}

/// Merged and closed pull requests are hidden until asked for, as on GitHub.
impl Default for Filters {
    fn default() -> Self {
        Self {
            provider: None,
            has_run: false,
            blocked: false,
            open_only: true,
        }
    }
}

/// What a run is doing, from Herdr's snapshot when its agent is there, else
/// from the state its owner stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Starting,
    Working,
    NeedsInput,
    Idle,
    Done,
    Failed,
    Stopped,
}

impl Status {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Working => "working",
            Self::NeedsInput => "needs input",
            Self::Idle => "idle",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }

    pub(crate) fn of(run: &Run, live: Option<AgentStatus>) -> Self {
        match live {
            Some(AgentStatus::Working) => return Self::Working,
            Some(AgentStatus::Blocked) => return Self::NeedsInput,
            Some(AgentStatus::Idle) => return Self::Idle,
            Some(AgentStatus::Done) => return Self::Done,
            Some(AgentStatus::Unknown) | None => {}
        }
        match run.state {
            RunState::Provisioning | RunState::Starting => Self::Starting,
            RunState::Running => Self::Working,
            RunState::NeedsInput => Self::NeedsInput,
            RunState::Idle => Self::Idle,
            RunState::Completed => Self::Done,
            RunState::Failed | RunState::Disconnected => Self::Failed,
            RunState::Cancelled => Self::Stopped,
        }
    }
}

/// An agent Herdr shows on one of the client's connected hosts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveAgent {
    /// The endpoint it is on, by index in the window's list.
    pub(crate) endpoint: usize,
    /// `None` on this machine, else the SSH destination.
    pub(crate) host: Option<String>,
    pub(crate) session: Option<String>,
    pub(crate) workspace_id: String,
    pub(crate) pane_id: String,
    pub(crate) status: AgentStatus,
    /// The agent's own summary of what it is doing, if it gave one.
    pub(crate) title: Option<String>,
}

/// The live agent a run's session names, if a connected host shows it.
pub(crate) fn live_for<'a>(session: &HerdrSession, live: &'a [LiveAgent]) -> Option<&'a LiveAgent> {
    live.iter().find(|agent| {
        agent.host == session.host
            && agent.session == session.session
            && agent.pane_id == session.pane_id
            && agent.workspace_id == session.workspace_id
    })
}

/// One run as listed: its index in the snapshot and its status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RunRow {
    pub(crate) run: usize,
    pub(crate) status: Status,
}

/// Runs joined with their sessions and live agents.
pub(crate) struct Runs<'a> {
    pub(crate) runs: &'a [Run],
    sessions: HashMap<&'a str, &'a HerdrSession>,
    live: &'a [LiveAgent],
}

impl<'a> Runs<'a> {
    pub(crate) fn new(
        runs: &'a [Run],
        sessions: &'a [HerdrSession],
        live: &'a [LiveAgent],
    ) -> Self {
        Self {
            runs,
            sessions: sessions
                .iter()
                .map(|session| (session.run_id.as_str(), session))
                .collect(),
            live,
        }
    }

    pub(crate) fn session(&self, run: usize) -> Option<&'a HerdrSession> {
        self.sessions.get(self.runs.get(run)?.id.as_str()).copied()
    }

    /// The live agent of `run`: by its Herdr session when its owner wrote
    /// one, else by the Herdr workspace the run recorded, which runs from
    /// before the shared sessions table still have.
    pub(crate) fn live(&self, run: usize) -> Option<&'a LiveAgent> {
        if let Some(session) = self.session(run) {
            return live_for(session, self.live);
        }
        let workspace = self
            .runs
            .get(run)?
            .workspace
            .as_ref()
            .filter(|workspace| workspace.backend == Backend::Herdr)?;
        self.live.iter().find(|agent| {
            agent.host == workspace.host
                && agent.workspace_id == workspace.id
                && if self.runs[run].owner == crate::orchestrator::Owner::Branch {
                    self.runs[run].id
                        == super::super::naming::workspace_run_id(
                            &agent.host,
                            &agent.session,
                            &agent.workspace_id,
                        )
                } else {
                    agent.session.is_none()
                }
        })
    }

    pub(crate) fn status(&self, run: usize) -> Option<Status> {
        let found = self.runs.get(run)?;
        Some(Status::of(found, self.live(run).map(|agent| agent.status)))
    }

    /// Every run of the item with canonical key `key`, newest first.
    pub(crate) fn of_item(&self, key: &str) -> Vec<RunRow> {
        let mut rows: Vec<_> = self
            .runs
            .iter()
            .enumerate()
            .filter(|(_, run)| run.item_key == key)
            .filter_map(|(index, _)| {
                Some(RunRow {
                    run: index,
                    status: self.status(index)?,
                })
            })
            .collect();
        rows.sort_by(|left, right| {
            self.runs[right.run]
                .started_at
                .cmp(&self.runs[left.run].started_at)
        });
        rows
    }

    /// The newest run of each item key.
    fn latest(&self) -> HashMap<&'a str, RunRow> {
        let mut latest: HashMap<&str, RunRow> = HashMap::new();
        for (index, run) in self.runs.iter().enumerate() {
            let Some(status) = self.status(index) else {
                continue;
            };
            let newer = latest
                .get(run.item_key.as_str())
                .is_none_or(|row| self.runs[row.run].started_at < run.started_at);
            if newer {
                latest.insert(&run.item_key, RunRow { run: index, status });
            }
        }
        latest
    }
}

/// One line of the Issues or Pull requests list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ItemRow {
    /// The repository listing it, by its place in the view's list.
    pub(crate) repo: usize,
    /// The item, by its place in that repository's snapshot.
    pub(crate) item: usize,
    pub(crate) depth: u8,
    /// `Some(open)` when the item has children shown under it.
    pub(crate) children: Option<bool>,
    pub(crate) run: Option<RunRow>,
}

/// Whether `item` matches every word of `query`, case-insensitively unless
/// a word has a capital, across the fields a user would search by.
pub(crate) fn matches(item: &Item, query: &str) -> bool {
    let fields = [
        Some(item.identifier.as_str()),
        Some(item.title.as_str()),
        Some(item.state.as_str()),
        item.author.as_deref(),
        item.description.as_deref(),
        Some(item.key.native_id.as_str()),
        item.pull_request.as_ref().map(|pr| pr.head_ref.as_str()),
    ];
    let text: String = fields
        .into_iter()
        .flatten()
        .chain(item.labels.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let lower = text.to_lowercase();
    query.split_whitespace().all(|word| {
        if word.chars().any(char::is_uppercase) {
            text.contains(word)
        } else {
            lower.contains(word)
        }
    })
}

pub(crate) fn closed(item: &Item) -> bool {
    matches!(item.state.as_str(), "closed" | "tombstone")
}

/// The Issues tab: open issues and beads, children under their parents.
pub(crate) fn issue_rows(
    items: &[Item],
    runs: &Runs<'_>,
    query: &str,
    filters: Filters,
    sort: Sort,
    collapsed: &HashSet<String>,
) -> Vec<ItemRow> {
    let latest = runs.latest();
    let keys: Vec<String> = items.iter().map(|item| item.key.canonical()).collect();
    let sources: Vec<String> = items
        .iter()
        .map(|item| item.key.source.canonical())
        .collect();
    let listed: Vec<usize> = (0..items.len())
        .filter(|&index| items[index].pull_request.is_none() && !closed(&items[index]))
        .collect();
    // A bead's parent is another bead of the same source with that id.
    let by_id: HashMap<(&str, &str), usize> = listed
        .iter()
        .map(|&index| {
            (
                (sources[index].as_str(), items[index].key.native_id.as_str()),
                index,
            )
        })
        .collect();
    let parent_of = |index: usize| -> Option<usize> {
        let parent = items[index].parent_id.as_deref()?;
        by_id
            .get(&(sources[index].as_str(), parent))
            .copied()
            .filter(|&found| found != index)
    };
    let own_match = |index: usize| {
        let item = &items[index];
        let run = latest.get(keys[index].as_str());
        matches(item, query)
            && filters
                .provider
                .is_none_or(|provider| item.key.source.provider == provider)
            && (!filters.has_run || run.is_some())
            && (!filters.blocked || !item.blocked_by.is_empty() || item.state == "blocked")
    };
    // Matches, plus every ancestor of one, so a matching child keeps its place.
    let mut shown: HashSet<usize> = HashSet::new();
    for &index in &listed {
        if own_match(index) {
            let mut at = Some(index);
            let mut guard = 0;
            while let Some(current) = at {
                if !shown.insert(current) || guard > 64 {
                    break;
                }
                at = parent_of(current);
                guard += 1;
            }
        }
    }
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for &index in &listed {
        if !shown.contains(&index) {
            continue;
        }
        match parent_of(index).filter(|parent| shown.contains(parent)) {
            Some(parent) => children.entry(parent).or_default().push(index),
            None => roots.push(index),
        }
    }
    let order = |list: &mut Vec<usize>| list.sort_by(|&a, &b| sort.compare(&items[a], &items[b]));
    order(&mut roots);
    for list in children.values_mut() {
        order(list);
    }
    let mut rows = Vec::new();
    let mut stack: Vec<(usize, u8)> = roots.into_iter().rev().map(|index| (index, 0)).collect();
    let mut visited = HashSet::new();
    while let Some((index, depth)) = stack.pop() {
        if !visited.insert(index) {
            continue;
        }
        let kids = children.get(&index);
        // A search opens every branch it reaches into.
        let open = !collapsed.contains(&keys[index]) || !query.trim().is_empty();
        rows.push(ItemRow {
            repo: 0,
            item: index,
            depth,
            children: kids.map(|_| open),
            run: latest.get(keys[index].as_str()).copied(),
        });
        if let Some(kids) = kids.filter(|_| open) {
            stack.extend(kids.iter().rev().map(|&kid| (kid, depth.saturating_add(1))));
        }
    }
    rows
}

/// The Pull requests tab: open and draft ones unless `open_only` is off.
pub(crate) fn pull_request_rows(
    items: &[Item],
    runs: &Runs<'_>,
    query: &str,
    filters: Filters,
    login: Option<&str>,
    mine: bool,
    sort: Sort,
) -> Vec<ItemRow> {
    let latest = runs.latest();
    let mut rows: Vec<ItemRow> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.pull_request.is_some())
        .filter(|(_, item)| !filters.open_only || matches!(item.state.as_str(), "open" | "draft"))
        .filter(|(_, item)| {
            !mine || login.is_some_and(|login| item.author.as_deref() == Some(login))
        })
        .filter(|(_, item)| matches(item, query))
        .map(|(index, item)| ItemRow {
            repo: 0,
            item: index,
            depth: 0,
            children: None,
            run: latest.get(item.key.canonical().as_str()).copied(),
        })
        .filter(|row| !filters.has_run || row.run.is_some())
        .collect();
    rows.sort_by(|a, b| sort.compare(&items[a.item], &items[b.item]));
    rows
}

/// What a run's item is called when the item is not listed, such as a
/// closed issue: `#375` for a GitHub issue or pull request, else its id.
pub(crate) fn key_label(key: &str) -> String {
    let native = key
        .rsplit(':')
        .next()
        .unwrap_or(key)
        .replace("%3A", ":")
        .replace("%25", "%");
    let number = native.strip_prefix("pr/").unwrap_or(&native);
    if !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) {
        format!("#{number}")
    } else {
        native
    }
}

/// What a pull request waits on, which groups the list: a review asked of
/// the user first, then the user's own to fix, those ready to merge, the
/// rest, and closed ones last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Next {
    Review,
    Fix,
    Merge,
    Wait,
    Closed,
}

impl Next {
    pub(crate) fn of(item: &Item, status: Option<&PrStatus>, login: Option<&str>) -> Self {
        if matches!(item.state.as_str(), "closed" | "merged") {
            return Self::Closed;
        }
        let mine = login.is_some_and(|login| {
            item.author
                .as_deref()
                .is_some_and(|author| author.eq_ignore_ascii_case(login))
        });
        let review = status.and_then(|status| status.review);
        let checks = status.and_then(PrStatus::checks_state);
        if !mine
            && status
                .zip(login)
                .is_some_and(|(status, login)| status.requests(login))
        {
            Self::Review
        } else if mine
            && (review == Some(Review::ChangesRequested) || checks == Some(CheckState::Failed))
        {
            Self::Fix
        } else if review == Some(Review::Approved)
            && !matches!(checks, Some(CheckState::Failed | CheckState::Pending))
            && item.state == "open"
        {
            Self::Merge
        } else {
            Self::Wait
        }
    }

    pub(crate) const ALL: [Self; 5] = [
        Self::Review,
        Self::Fix,
        Self::Merge,
        Self::Wait,
        Self::Closed,
    ];
}

/// What heads a run of rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Heading {
    Next(Next),
    /// An issue's priority; None for none.
    Priority(Option<i64>),
}

/// One line of a section's list: a heading, or an item's row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Line {
    Heading { heading: Heading, count: usize },
    Row(ItemRow),
}

/// The status GitHub last gave pull request `item`, if it is one.
pub(crate) fn status_of<'a>(
    item: &Item,
    statuses: &'a HashMap<(String, u64), PrStatus>,
) -> Option<&'a PrStatus> {
    let number = item.pull_request.as_ref()?.number;
    statuses.get(&(item.key.source.repository.clone(), number))
}

/// Several repositories' rows as one list in `sort`'s order. A top-level
/// row keeps the children that follow it.
pub(crate) fn merge<'a>(
    lists: impl IntoIterator<Item = Vec<ItemRow>>,
    item: impl Fn(&ItemRow) -> &'a Item,
    sort: Sort,
) -> Vec<ItemRow> {
    let mut blocks: Vec<Vec<ItemRow>> = Vec::new();
    for row in lists.into_iter().flatten() {
        match blocks.last_mut() {
            Some(block) if row.depth > 0 => block.push(row),
            _ => blocks.push(vec![row]),
        }
    }
    blocks.sort_by(|a, b| sort.compare(item(&a[0]), item(&b[0])));
    blocks.into_iter().flatten().collect()
}

/// One row per item: an issue or pull request several checkouts list (the
/// same repository on two hosts) shows once, where its first copy came, and
/// as the copy with a run when only another has one. Also returns, by item
/// key, every repository listing it, in the order they came.
pub(crate) fn dedupe(
    rows: Vec<ItemRow>,
    key: impl Fn(&ItemRow) -> String,
) -> (Vec<ItemRow>, HashMap<String, Vec<usize>>) {
    let mut kept: Vec<ItemRow> = Vec::with_capacity(rows.len());
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut holders: HashMap<String, Vec<usize>> = HashMap::new();
    for row in rows {
        let key = key(&row);
        let repos = holders.entry(key.clone()).or_default();
        if !repos.contains(&row.repo) {
            repos.push(row.repo);
        }
        match at.get(&key) {
            Some(&index) => {
                let first = &mut kept[index];
                if first.run.is_none() && row.run.is_some() {
                    // The tree shape stays the first copy's: its children
                    // follow it.
                    *first = ItemRow {
                        depth: first.depth,
                        children: first.children,
                        ..row
                    };
                }
            }
            None => {
                at.insert(key, kept.len());
                kept.push(row);
            }
        }
    }
    (kept, holders)
}

/// Pull requests under what each waits on, in the order the rows came.
pub(crate) fn pull_request_lines(rows: &[ItemRow], next: impl Fn(&ItemRow) -> Next) -> Vec<Line> {
    let mut lines = Vec::new();
    for group in Next::ALL {
        let grouped: Vec<ItemRow> = rows
            .iter()
            .copied()
            .filter(|row| next(row) == group)
            .collect();
        if grouped.is_empty() {
            continue;
        }
        lines.push(Line::Heading {
            heading: Heading::Next(group),
            count: grouped.len(),
        });
        lines.extend(grouped.into_iter().map(Line::Row));
    }
    lines
}

/// Issues under their priority, hottest first and none last. A Beads child
/// stays under its parent, in its parent's group.
pub(crate) fn issue_lines<'a>(rows: &[ItemRow], item: impl Fn(&ItemRow) -> &'a Item) -> Vec<Line> {
    // Each top-level row with the children that follow it.
    let mut blocks: Vec<(Option<i64>, Vec<ItemRow>)> = Vec::new();
    for row in rows {
        match blocks.last_mut() {
            Some((_, block)) if row.depth > 0 => block.push(*row),
            _ => blocks.push((item(row).priority, vec![*row])),
        }
    }
    let mut priorities: Vec<Option<i64>> = blocks.iter().map(|(priority, _)| *priority).collect();
    priorities.sort_by_key(|priority| (priority.is_none(), *priority));
    priorities.dedup();
    let mut lines = Vec::new();
    for priority in priorities {
        let inside: Vec<ItemRow> = blocks
            .iter()
            .filter(|(of, _)| *of == priority)
            .flat_map(|(_, block)| block.iter().copied())
            .collect();
        lines.push(Line::Heading {
            heading: Heading::Priority(priority),
            count: inside.iter().filter(|row| row.depth == 0).count(),
        });
        lines.extend(inside.into_iter().map(Line::Row));
    }
    lines
}
