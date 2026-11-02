//! What the Overview lists, gathered from the window's prepared state before
//! drawing: every connected host's agents and the open orchestrators' failed
//! runs, each with a key that outlives a daemon restart, and the page's
//! sections once the user's marks sort them.

use super::marks::{Hold, Marks};
use crate::{
    endpoint::Endpoint,
    notification_mutes::{Daemon, Mutes},
    orchestrator::ViewId,
    state::LiveState,
};
use herdr_client::protocol::{AgentStatus, ClientShellAgent, ClientShellSnapshot};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::SystemTime};

/// Untrusted titles are cut here: a row has one line for them.
const TITLE_LIMIT: usize = 200;

/// What a row stands for, as the user's marks remember it. Pane and
/// workspace ids do not survive a daemon restart, so an agent is the host,
/// the checkout it works in, and its name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Key {
    Agent {
        endpoint: String,
        checkout: String,
        agent: String,
    },
    Run {
        id: String,
    },
}

impl Key {
    /// Text a selector can carry.
    pub(crate) fn selector(&self) -> String {
        match self {
            Self::Agent {
                endpoint,
                checkout,
                agent,
            } => format!("{endpoint}/{checkout}/{agent}"),
            Self::Run { id } => format!("run/{id}"),
        }
    }

    pub(crate) fn fields(&self) -> impl Iterator<Item = &str> {
        let fields: [&str; 3] = match self {
            Self::Agent {
                endpoint,
                checkout,
                agent,
            } => [endpoint, checkout, agent],
            Self::Run { id } => [id, "", ""],
        };
        fields.into_iter()
    }
}

/// Where a row stands, most urgent first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum State {
    Blocked,
    Failed,
    Working,
    Done,
    Idle,
}

impl State {
    pub(crate) fn of(status: AgentStatus) -> Self {
        match status {
            AgentStatus::Blocked => Self::Blocked,
            AgentStatus::Working => Self::Working,
            AgentStatus::Done => Self::Done,
            AgentStatus::Idle | AgentStatus::Unknown => Self::Idle,
        }
    }

    /// Whether the row asks for the user.
    pub(crate) fn needs_you(self) -> bool {
        matches!(self, Self::Blocked | Self::Failed)
    }

    /// The daemon's status a row of this state shows a dot for.
    pub(crate) fn status(self) -> AgentStatus {
        match self {
            Self::Blocked | Self::Failed => AgentStatus::Blocked,
            Self::Working => AgentStatus::Working,
            Self::Done => AgentStatus::Done,
            Self::Idle => AgentStatus::Idle,
        }
    }
}

/// What clicking a row shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Open {
    Pane {
        endpoint: String,
        pane: String,
        tab: String,
        workspace: String,
    },
    /// The orchestrator view that lists the run.
    Run(ViewId),
}

/// A checkout's pull request as the prefetch last saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pr {
    pub number: u64,
    pub checks: Option<crate::pull_request::Outcome>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub key: Key,
    pub state: State,
    /// What the agent says it does, or the run's item.
    pub title: String,
    /// The agent's name and status words, or why the run stopped.
    pub detail: String,
    /// The agent's identity, for its icon; None for a run.
    pub identity: Option<String>,
    pub workspace: String,
    pub branch: Option<String>,
    /// The device's label.
    pub host: String,
    /// The device's endpoint id, which groups working rows by host.
    pub endpoint: String,
    /// When the row entered its state, as far as this client saw it.
    pub since: Option<SystemTime>,
    pub pr: Option<Pr>,
    pub open: Open,
    /// The agent's workspace or tab is muted: listed, but counting for
    /// nothing in the activity bar.
    pub muted: bool,
}

/// When each agent entered its state. The daemon reports the order of
/// changes, not their time, so the time is when this client first saw it.
#[derive(Debug, Default)]
pub(crate) struct Since {
    seen: HashMap<(String, String), (AgentStatus, SystemTime)>,
}

impl Since {
    /// Records the states of `endpoint`'s agents, keeping the time of those
    /// that did not change.
    pub(crate) fn observe(&mut self, endpoint: &str, agents: &[ClientShellAgent], now: SystemTime) {
        for agent in agents {
            let key = (endpoint.to_owned(), agent.pane_id.clone());
            match self.seen.get_mut(&key) {
                Some(seen) if seen.0 == agent.agent_status => {}
                Some(seen) => *seen = (agent.agent_status, now),
                None => {
                    self.seen.insert(key, (agent.agent_status, now));
                }
            }
        }
    }

    /// Forgets agents of hosts not in `endpoints`, and agents gone from a
    /// host that reported.
    pub(crate) fn retain(&mut self, live: &HashMap<&str, Vec<&str>>) {
        self.seen.retain(|(endpoint, pane), _| {
            live.get(endpoint.as_str())
                .is_some_and(|panes| panes.contains(&pane.as_str()))
        });
    }

    pub(crate) fn get(&self, endpoint: &str, pane: &str) -> Option<SystemTime> {
        self.seen
            .get(&(endpoint.to_owned(), pane.to_owned()))
            .map(|(_, at)| *at)
    }
}

/// The checkout an agent works in: its worktree and branch, else its
/// workspace's name.
fn checkout(
    snapshot: &ClientShellSnapshot,
    agent: &ClientShellAgent,
) -> (String, String, Option<String>) {
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == agent.workspace_id);
    let label = workspace
        .map(|workspace| workspace.label.clone())
        .unwrap_or_default();
    let branch = workspace.and_then(|workspace| workspace.branch.clone());
    let key = workspace
        .and_then(|workspace| Some((workspace.worktree.as_ref()?, workspace.branch.as_deref()?)))
        .map(|(tree, branch)| format!("{}@{branch}", tree.key))
        .unwrap_or_else(|| label.clone());
    (key, label, branch)
}

/// What an agent says it does: its pane's title, else its name.
fn title(agent: &ClientShellAgent, name: &str) -> String {
    let title = agent
        .terminal_title_stripped
        .as_deref()
        .or(agent.terminal_title.as_deref())
        .map(|title| crate::notifications::safe_text(title, TITLE_LIMIT))
        .map(|title| title.trim().to_owned())
        .filter(|title| !title.is_empty() && title != name);
    title.unwrap_or_else(|| name.to_owned())
}

/// The rows of one reachable device's agents, in the daemon's order. A
/// device out of reach lists none rather than stale ones.
pub(crate) fn agent_rows(
    endpoint: &Endpoint,
    live: &LiveState,
    since: &Since,
    pr: impl Fn(&str, &str) -> Option<Pr>,
    mutes: Option<&Mutes>,
) -> Vec<Row> {
    if !endpoint.enabled || !live.status.is_connected() {
        return Vec::new();
    }
    let Some(snapshot) = live.snapshot.as_deref() else {
        return Vec::new();
    };
    let silenced = mutes
        .map(|mutes| mutes.silenced(&Daemon::of(endpoint), snapshot))
        .unwrap_or_default();
    snapshot
        .agents
        .iter()
        .map(|agent| {
            let name = crate::sidebar::agent_name(agent);
            let (checkout, workspace, branch) = checkout(snapshot, agent);
            let status = crate::sidebar::status_text(agent.agent_status);
            let label = crate::sidebar::state_label(agent, &status);
            let detail = if label == status {
                name.to_owned()
            } else {
                format!("{name} · {label}")
            };
            let pr = snapshot
                .workspaces
                .iter()
                .find(|shown| shown.workspace_id == agent.workspace_id)
                .and_then(|shown| Some((shown.worktree.as_ref()?, shown.branch.as_deref()?)))
                .and_then(|(tree, branch)| pr(&tree.key, branch));
            Row {
                key: Key::Agent {
                    endpoint: endpoint.id.clone(),
                    checkout,
                    agent: name.to_owned(),
                },
                state: State::of(agent.agent_status),
                title: title(agent, name),
                detail,
                identity: agent.agent.clone(),
                workspace,
                branch,
                host: endpoint.label.clone(),
                endpoint: endpoint.id.clone(),
                since: since.get(&endpoint.id, &agent.pane_id),
                pr,
                open: Open::Pane {
                    endpoint: endpoint.id.clone(),
                    pane: agent.pane_id.clone(),
                    tab: agent.tab_id.clone(),
                    workspace: agent.workspace_id.clone(),
                },
                muted: silenced.notice(
                    Some(&agent.workspace_id),
                    Some(&agent.tab_id),
                    Some(&agent.pane_id),
                ),
            }
        })
        .collect()
}

/// The page's sections, each in the order rows arrived.
#[derive(Debug, Default)]
pub(crate) struct Sections {
    pub pinned: Vec<Row>,
    pub needs: Vec<Row>,
    pub working: Vec<Row>,
    pub ready: Vec<Row>,
    pub idle: Vec<Row>,
    /// With when each wakes.
    pub snoozed: Vec<(Row, SystemTime)>,
    pub settled: Vec<Row>,
}

impl Sections {
    /// Sorts `rows` by the user's marks, then by state: a held row is out of
    /// the way until its hold ends, a pinned one leads.
    pub(crate) fn of(rows: Vec<Row>, marks: Option<&Marks>, now: SystemTime) -> Self {
        let mut sections = Self::default();
        for row in rows {
            let mark = marks.and_then(|marks| marks.get(&row.key));
            match mark.and_then(|mark| mark.hold(row.state, now)) {
                Some(Hold::Snoozed { until }) => sections.snoozed.push((row, until)),
                Some(Hold::Settled) => sections.settled.push(row),
                None if mark.is_some_and(|mark| mark.pinned) => sections.pinned.push(row),
                None => match row.state {
                    State::Blocked | State::Failed => sections.needs.push(row),
                    State::Working => sections.working.push(row),
                    State::Done => sections.ready.push(row),
                    State::Idle => sections.idle.push(row),
                },
            }
        }
        sections.snoozed.sort_by_key(|(_, until)| *until);
        sections
    }

    /// Whether no row is in any section.
    pub(crate) fn is_empty(&self) -> bool {
        self.pinned.is_empty()
            && self.needs.is_empty()
            && self.working.is_empty()
            && self.ready.is_empty()
            && self.idle.is_empty()
            && self.snoozed.is_empty()
            && self.settled.is_empty()
    }

    /// The rows the activity bar counts: those that need the user and are
    /// neither held nor muted.
    fn counted(&self) -> impl Iterator<Item = &Row> {
        self.needs
            .iter()
            .chain(self.pinned.iter().filter(|row| row.state.needs_you()))
            .filter(|row| !row.muted)
    }

    /// What the activity bar counts.
    pub(crate) fn needing(&self) -> usize {
        self.counted().count()
    }

    /// When the latest of the rows [`Self::needing`] counts entered its
    /// state.
    pub(crate) fn needing_since(&self) -> Option<SystemTime> {
        self.counted().filter_map(|row| row.since).max()
    }
}
