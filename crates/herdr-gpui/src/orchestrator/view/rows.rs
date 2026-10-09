//! What the inbox lists, derived from a snapshot without drawing: issues with
//! Beads children under their parents, pull requests, and runs grouped by
//! what needs the user first. Pure, so it is tested without a window.

use crate::orchestrator::{Backend, HerdrSession, Item, Provider, Run, RunState};
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
    Runs,
}

impl Tab {
    pub(crate) const ALL: [Self; 3] = [Self::Issues, Self::PullRequests, Self::Runs];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Issues => "Issues",
            Self::PullRequests => "Pull requests",
            Self::Runs => "Runs",
        }
    }
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

    fn compare(self, left: &Item, right: &Item) -> Ordering {
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Filters {
    /// Only one provider's items.
    pub(crate) provider: Option<Provider>,
    pub(crate) has_run: bool,
    pub(crate) blocked: bool,
    /// Pull requests: only open and draft ones.
    pub(crate) open_only: bool,
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

    /// The Runs tab's groups, most urgent first.
    fn group(self) -> Group {
        match self {
            Self::NeedsInput | Self::Failed => Group::Attention,
            Self::Starting | Self::Working => Group::Working,
            Self::Idle => Group::Idle,
            Self::Done | Self::Stopped => Group::Finished,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Group {
    Attention,
    Working,
    Idle,
    Finished,
}

impl Group {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Attention => "NEEDS YOU",
            Self::Working => "WORKING",
            Self::Idle => "IDLE",
            Self::Finished => "FINISHED",
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
        self.live
            .iter()
            .find(|agent| agent.host == workspace.host && agent.workspace_id == workspace.id)
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ItemRow {
    pub(crate) item: usize,
    pub(crate) depth: u8,
    /// `Some(open)` when the item has children shown under it.
    pub(crate) children: Option<bool>,
    pub(crate) run: Option<RunRow>,
}

/// One line of the Runs list: a group heading or a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RunLine {
    Group { group: Group, count: usize },
    Run(RunRow),
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

fn closed(item: &Item) -> bool {
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

/// The Pull requests tab, in every state unless `open_only`.
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

/// The Runs tab: runs under group headings, newest first in each.
pub(crate) fn run_lines(
    runs: &Runs<'_>,
    items: &[Item],
    query: &str,
    active_only: bool,
) -> Vec<RunLine> {
    let titles: HashMap<String, &Item> = items
        .iter()
        .map(|item| (item.key.canonical(), item))
        .collect();
    let mut rows: Vec<RunRow> = (0..runs.runs.len())
        .filter_map(|index| {
            Some(RunRow {
                run: index,
                status: runs.status(index)?,
            })
        })
        .filter(|row| !active_only || row.status.group() != Group::Finished)
        .filter(|row| {
            let run = &runs.runs[row.run];
            let haystack = format!(
                "{} {} {} {}",
                run.agent,
                run.item_key,
                titles
                    .get(&run.item_key)
                    .map_or("", |item| item.title.as_str()),
                run.workspace
                    .as_ref()
                    .map_or("", |workspace| workspace.branch.as_str()),
            );
            query
                .split_whitespace()
                .all(|word| haystack.to_lowercase().contains(&word.to_lowercase()))
        })
        .collect();
    rows.sort_by(|a, b| {
        a.status.group().cmp(&b.status.group()).then_with(|| {
            runs.runs[b.run]
                .started_at
                .cmp(&runs.runs[a.run].started_at)
        })
    });
    let mut lines = Vec::new();
    let mut index = 0;
    while index < rows.len() {
        let group = rows[index].status.group();
        let count = rows[index..]
            .iter()
            .take_while(|row| row.status.group() == group)
            .count();
        lines.push(RunLine::Group { group, count });
        lines.extend(rows[index..index + count].iter().copied().map(RunLine::Run));
        index += count;
    }
    lines
}

/// How many runs need the user: blocked on input or failed. The Runs tab and
/// the entry points mark it.
pub(crate) fn attention(runs: &Runs<'_>) -> usize {
    (0..runs.runs.len())
        .filter_map(|index| runs.status(index))
        .filter(|status| status.group() == Group::Attention)
        .count()
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
