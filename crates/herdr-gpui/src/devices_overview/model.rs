//! What the overview shows for one device, gathered from its endpoint and
//! live state before drawing, and the search that narrows the list.

use super::history::Counts;
use crate::{endpoint::Endpoint, state::LiveState, usage::Host};
use herdr_client::{ConnectTarget, protocol::AgentStatus};

/// A device's agents by state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Tally {
    pub working: usize,
    pub blocked: usize,
    pub done: usize,
    pub idle: usize,
}

impl Tally {
    pub fn of(statuses: impl IntoIterator<Item = AgentStatus>) -> Self {
        let mut tally = Self::default();
        for status in statuses {
            match status {
                AgentStatus::Working => tally.working += 1,
                AgentStatus::Blocked => tally.blocked += 1,
                AgentStatus::Done => tally.done += 1,
                AgentStatus::Idle | AgentStatus::Unknown => tally.idle += 1,
            }
        }
        tally
    }

    pub fn total(self) -> usize {
        self.working + self.blocked + self.done + self.idle
    }

    pub fn counts(self) -> Counts {
        let clamp = |n: usize| u16::try_from(n).unwrap_or(u16::MAX);
        Counts {
            working: clamp(self.working),
            blocked: clamp(self.blocked),
        }
    }

    /// Every device's agents together.
    pub fn sum<'a>(devices: impl IntoIterator<Item = &'a Device>) -> Self {
        devices
            .into_iter()
            .fold(Self::default(), |sum, device| sum.add(device.tally))
    }

    pub fn add(self, other: Self) -> Self {
        Self {
            working: self.working + other.working,
            blocked: self.blocked + other.blocked,
            done: self.done + other.done,
            idle: self.idle + other.idle,
        }
    }
}

/// A reachable device's agents, counted straight from its snapshot, or None
/// while it cannot be reached. The tick and the sidebar's headers need only
/// this, not a whole [`Device`].
pub(crate) fn online_tally(endpoint: &Endpoint, live: &LiveState) -> Option<Tally> {
    if !endpoint.enabled || !live.status.is_connected() {
        return None;
    }
    let snapshot = live.snapshot.as_deref()?;
    Some(Tally::of(
        snapshot.agents.iter().map(|agent| agent.agent_status),
    ))
}

/// Whether a device can be reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Link {
    Online,
    Connecting,
    Offline,
    Disabled,
}

/// One agent as an expanded device row lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentLine {
    pub pane_id: String,
    /// The Herdr tab and workspace holding the pane, so a click can show
    /// that tab even when the daemon's focus does not move.
    pub tab_id: String,
    pub workspace_id: String,
    pub name: String,
    /// The agent's identity, for its icon.
    pub identity: Option<String>,
    pub workspace: String,
    pub status: AgentStatus,
}

/// The most agent dots a device's row draws; the count says the rest.
pub(crate) const DOTS: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Device {
    pub index: usize,
    pub id: String,
    pub label: String,
    /// How the device is reached: `ssh mac-studio`, `WSL · Ubuntu`.
    pub address: String,
    pub link: Link,
    pub tally: Tally,
    /// The first [`DOTS`] agents' states, working first.
    pub dots: Vec<AgentStatus>,
    /// Every agent, working first, for an expanded row.
    pub agents: Vec<AgentLine>,
    /// Agent and workspace names, lowercased, for the search.
    names: Vec<String>,
    /// Where load samples come from; None for a cloud machine.
    pub host: Option<Host>,
}

impl Device {
    pub(crate) fn new(index: usize, endpoint: &Endpoint, live: &LiveState) -> Self {
        let link = if !endpoint.enabled {
            Link::Disabled
        } else if live.status.is_connected() {
            Link::Online
        } else if live.error.is_some()
            || matches!(
                live.status,
                crate::state::ConnectionStatus::Disconnected
                    | crate::state::ConnectionStatus::Detached
            )
        {
            Link::Offline
        } else {
            Link::Connecting
        };
        // A device out of reach shows no agents rather than stale ones.
        let snapshot = live.snapshot.as_deref().filter(|_| link == Link::Online);
        let agents = snapshot.map_or(&[][..], |snapshot| &snapshot.agents[..]);
        let mut lines: Vec<AgentLine> = agents
            .iter()
            .map(|agent| AgentLine {
                pane_id: agent.pane_id.clone(),
                tab_id: agent.tab_id.clone(),
                workspace_id: agent.workspace_id.clone(),
                name: crate::sidebar::agent_name(agent).to_owned(),
                identity: agent.agent.clone(),
                workspace: snapshot
                    .and_then(|snapshot| {
                        snapshot
                            .workspaces
                            .iter()
                            .find(|workspace| workspace.workspace_id == agent.workspace_id)
                    })
                    .map(|workspace| workspace.label.clone())
                    .unwrap_or_default(),
                status: agent.agent_status,
            })
            .collect();
        // Stable, so agents in one state keep the daemon's order.
        lines.sort_by_key(|line| order(line.status));
        let dots = lines.iter().take(DOTS).map(|line| line.status).collect();
        let mut names: Vec<String> = agents
            .iter()
            .map(|agent| crate::sidebar::agent_name(agent).to_lowercase())
            .collect();
        if let Some(snapshot) = snapshot {
            names.extend(
                snapshot
                    .workspaces
                    .iter()
                    .map(|workspace| workspace.label.to_lowercase()),
            );
        }
        Self {
            index,
            id: endpoint.id.clone(),
            label: endpoint.label.clone(),
            address: address(&endpoint.connection.target),
            link,
            tally: Tally::of(agents.iter().map(|agent| agent.agent_status)),
            dots,
            agents: lines,
            names,
            host: Host::of(&endpoint.connection.target),
        }
    }

    /// Whether every word of `query` names this device, how it is reached,
    /// or one of its agents or workspaces.
    pub(crate) fn matches(&self, query: &str) -> bool {
        let fields = [self.label.to_lowercase(), self.address.to_lowercase()];
        query.split_whitespace().all(|word| {
            let word = word.to_lowercase();
            fields
                .iter()
                .chain(&self.names)
                .any(|field| field.contains(&word))
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(label: &str, link: Link, statuses: &[AgentStatus]) -> Self {
        let mut dots = statuses.to_vec();
        dots.sort_by_key(|status| order(*status));
        dots.truncate(DOTS);
        Self {
            index: 0,
            id: label.to_owned(),
            label: label.to_owned(),
            address: format!("ssh {label}"),
            link,
            tally: Tally::of(statuses.iter().copied()),
            dots,
            agents: Vec::new(),
            names: vec!["claude".into(), "herdr-gpui".into()],
            host: None,
        }
    }
}

/// Working agents first, then those waiting, then finished and idle ones.
fn order(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Working => 0,
        AgentStatus::Blocked => 1,
        AgentStatus::Done => 2,
        AgentStatus::Idle => 3,
        AgentStatus::Unknown => 4,
    }
}

/// How a device is reached, as its row's second line says it.
pub(crate) fn address(target: &ConnectTarget) -> String {
    match target {
        ConnectTarget::Local => "this machine".to_owned(),
        ConnectTarget::Session { name, .. } => format!("session {name}"),
        ConnectTarget::Socket(path) => path.display().to_string(),
        ConnectTarget::Ssh { target, .. } => format!("ssh {target}"),
        #[cfg(feature = "cloud")]
        ConnectTarget::Cloud {
            provider, machine, ..
        } => format!("{} · {machine}", provider.key()),
        ConnectTarget::Wsl { distro, .. } => format!("WSL · {distro}"),
    }
}
