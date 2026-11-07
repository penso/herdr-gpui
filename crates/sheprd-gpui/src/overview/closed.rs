//! Agents that left a host while it stayed up: their pane or tab closed, or
//! the agent exited. The daemon forgets them at once, so the page keeps the
//! last row it drew of each, with the folder it ran in, to list under Closed
//! and to start again (`reopen`). The window keeps them while it is open. A
//! daemon restart is not a close: a new boot id starts the host afresh.

use super::model::{Key, Open, Row};
use sheprd_client::protocol::ClientShellSnapshot;
use std::{collections::HashMap, time::SystemTime};

/// Closed agents kept; the oldest go first beyond this.
const MAX_CLOSED: usize = 32;

/// An agent that closed, as it last showed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Agent {
    /// The daemon boot that owns the saved workspace and pane IDs.
    pub boot: String,
    /// Tells apart agents that share a key, such as two of one kind in one
    /// checkout, for Reopen and Forget.
    pub id: u64,
    pub row: Row,
    /// The folder its pane last ran in, for the tab that reopens it.
    pub cwd: Option<String>,
    pub closed: SystemTime,
}

/// One host's agents as its last report listed them.
#[derive(Debug)]
struct Seen {
    boot: String,
    agents: Vec<(Row, Option<String>)>,
}

#[derive(Debug, Default)]
pub(crate) struct Closed {
    /// By endpoint id.
    seen: HashMap<String, Seen>,
    /// Oldest first.
    agents: Vec<Agent>,
    /// Counts changes, so the page redraws when one closes or goes.
    revision: u64,
    /// The next agent's `id`.
    next: u64,
    /// Reopened agents not yet listed again, by endpoint and key: when one
    /// turns up, it is the agent reopened, not a closed twin coming back.
    reopened: Vec<(String, Key)>,
}

/// The pane an agent row stands in.
fn pane(row: &Row) -> Option<&str> {
    match &row.open {
        Open::Pane { pane, .. } => Some(pane),
        Open::Run(_) => None,
    }
}

/// The folder a row's pane runs in: its foreground process's, else the
/// shell's.
fn cwd(snapshot: &ClientShellSnapshot, row: &Row) -> Option<String> {
    let pane = snapshot
        .panes
        .iter()
        .find(|shown| Some(shown.pane_id.as_str()) == pane(row))?;
    [&pane.foreground_cwd, &pane.cwd]
        .into_iter()
        .flatten()
        .find(|path| !path.is_empty())
        .cloned()
}

impl Closed {
    /// Notes `endpoint`'s agents from its report `snapshot`: those listed on
    /// the last report of the same boot and gone from this one closed. An
    /// agent that turns up again, reopened or started anew, is no longer
    /// listed as closed.
    pub(crate) fn observe<'a>(
        &mut self,
        endpoint: &str,
        snapshot: &ClientShellSnapshot,
        rows: impl IntoIterator<Item = &'a Row>,
        now: SystemTime,
    ) {
        let current: Vec<(Row, Option<String>)> = rows
            .into_iter()
            .filter(|row| row.endpoint == endpoint && pane(row).is_some())
            .map(|row| (row.clone(), cwd(snapshot, row)))
            .collect();
        let seen = Seen {
            boot: snapshot.boot_id.clone(),
            agents: current,
        };
        let previous = self.seen.insert(endpoint.to_owned(), seen);
        if previous
            .as_ref()
            .is_some_and(|previous| previous.boot != snapshot.boot_id)
        {
            self.reopened.retain(|(host, _)| host != endpoint);
        }
        let previous = previous
            .filter(|previous| previous.boot == snapshot.boot_id)
            .map(|previous| previous.agents)
            .unwrap_or_default();
        let Some(current) = self.seen.get(endpoint).map(|seen| &seen.agents) else {
            return;
        };
        let before = self.agents.len();
        // Two agents of one name in one checkout share a key, so each agent
        // of a key new on this report brings back one closed agent of it,
        // the latest closed.
        let mut counted: Vec<&Key> = Vec::new();
        for (row, _) in current {
            if counted.contains(&&row.key) {
                continue;
            }
            counted.push(&row.key);
            let count = |agents: &[(Row, Option<String>)]| {
                agents
                    .iter()
                    .filter(|(shown, _)| shown.key == row.key)
                    .count()
            };
            let mut back = count(current).saturating_sub(count(&previous));
            while back > 0
                && let Some(index) = self
                    .reopened
                    .iter()
                    .position(|(host, key)| host == endpoint && *key == row.key)
            {
                self.reopened.remove(index);
                back -= 1;
            }
            while back > 0
                && let Some(index) = self.agents.iter().rposition(|agent| {
                    agent.boot == snapshot.boot_id
                        && agent.row.endpoint == endpoint
                        && agent.row.key == row.key
                })
            {
                self.agents.remove(index);
                back -= 1;
            }
        }
        let mut changed = self.agents.len() != before;
        // Another agent started in the pane, between two reports, still
        // closed the one before it.
        for (row, cwd) in previous {
            if current
                .iter()
                .any(|(shown, _)| pane(shown) == pane(&row) && shown.identity == row.identity)
            {
                continue;
            }
            if self.agents.len() >= MAX_CLOSED {
                self.agents.remove(0);
            }
            self.agents.push(Agent {
                boot: snapshot.boot_id.clone(),
                id: self.next,
                row,
                cwd,
                closed: now,
            });
            self.next += 1;
            changed = true;
        }
        if changed {
            self.revision += 1;
        }
    }

    /// Forgets hosts the window no longer has, and their closed agents.
    pub(crate) fn retain_hosts(&mut self, mut kept: impl FnMut(&str) -> bool) {
        self.seen.retain(|endpoint, _| kept(endpoint));
        self.reopened.retain(|(endpoint, _)| kept(endpoint));
        let before = self.agents.len();
        self.agents.retain(|agent| kept(&agent.row.endpoint));
        if self.agents.len() != before {
            self.revision += 1;
        }
    }

    /// Newest first, as the page lists them.
    pub(crate) fn agents(&self) -> impl Iterator<Item = &Agent> {
        self.agents.iter().rev()
    }

    pub(crate) fn len(&self) -> usize {
        self.agents.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    pub(crate) fn get(&self, id: u64) -> Option<&Agent> {
        self.agents.iter().find(|agent| agent.id == id)
    }

    pub(crate) fn forget(&mut self, id: u64) {
        let before = self.agents.len();
        self.agents.retain(|agent| agent.id != id);
        if self.agents.len() != before {
            self.revision += 1;
        }
    }

    /// Forgets the closed agent `id`, started again, so that its return
    /// does not take a twin closed beside it off the list.
    pub(crate) fn reopened(&mut self, id: u64) {
        let Some(reopened) = self
            .get(id)
            .map(|agent| (agent.row.endpoint.clone(), agent.row.key.clone()))
        else {
            return;
        };
        if self.reopened.len() >= MAX_CLOSED {
            self.reopened.remove(0);
        }
        self.reopened.push(reopened);
        self.forget(id);
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
}
