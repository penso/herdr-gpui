//! What a phone lists: the daemon's workspaces, each with its chats and its
//! terminal tabs, projected from a snapshot. Pure, so it is tested without
//! a daemon.

use herdr_chat_protocol::{EndpointId, TOKEN};
use herdr_client::protocol::{AgentStatus, ClientShellSnapshot};
use std::collections::HashSet;

/// The most chats one daemon may list; the rest are left out.
pub(crate) const MAX_CHATS: usize = 64;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Layout {
    /// Whether the Herdr daemon is reachable now.
    pub(crate) connected: bool,
    /// The boot the snapshot came from; requests act on it.
    pub(crate) boot: String,
    pub(crate) workspaces: Vec<Workspace>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Workspace {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) branch: Option<String>,
    /// The folder a new tab or chat in it starts in.
    pub(crate) cwd: String,
    pub(crate) chats: Vec<Chat>,
    pub(crate) terminals: Vec<Terminal>,
}

/// A pane running `herdr-acp`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Chat {
    pub(crate) pane: String,
    pub(crate) tab: String,
    pub(crate) endpoint: EndpointId,
    /// The agent's name as Herdr reports it, such as `claude`.
    pub(crate) agent: Option<String>,
    pub(crate) title: String,
    pub(crate) status: AgentStatus,
}

/// A tab with no chat in it: a terminal a phone shows one pane at a time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Terminal {
    pub(crate) tab: String,
    pub(crate) label: String,
    pub(crate) cwd: Option<String>,
    /// Its panes in the snapshot's order; never empty for a listed tab.
    pub(crate) panes: Vec<TerminalPane>,
    pub(crate) zoomed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TerminalPane {
    pub(crate) pane: String,
    pub(crate) label: Option<String>,
    pub(crate) cwd: Option<String>,
    /// The tab's focused pane, which a phone opens first.
    pub(crate) focused: bool,
}

impl Terminal {
    /// The pane a phone opens first: the focused one, else the first.
    pub(crate) fn first_pane(&self) -> Option<&TerminalPane> {
        self.panes
            .iter()
            .find(|pane| pane.focused)
            .or_else(|| self.panes.first())
    }
}

impl Layout {
    pub(crate) fn chat(&self, pane: &str) -> Option<(&Workspace, &Chat)> {
        self.workspaces.iter().find_map(|workspace| {
            workspace
                .chats
                .iter()
                .find(|chat| chat.pane == pane)
                .map(|chat| (workspace, chat))
        })
    }

    pub(crate) fn workspace(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| workspace.id == id)
    }

    /// The terminal tab holding `pane`, with its workspace.
    pub(crate) fn terminal(&self, pane: &str) -> Option<(&Workspace, &Terminal)> {
        self.workspaces.iter().find_map(|workspace| {
            workspace
                .terminals
                .iter()
                .find(|terminal| {
                    terminal
                        .panes
                        .iter()
                        .any(|candidate| candidate.pane == pane)
                })
                .map(|terminal| (workspace, terminal))
        })
    }

    pub(crate) fn chats(&self) -> impl Iterator<Item = &Chat> {
        self.workspaces
            .iter()
            .flat_map(|workspace| workspace.chats.iter())
    }
}

/// The program a pane's metadata names, when its token holds a valid id.
fn endpoint(tokens: &[(String, String)]) -> Option<EndpointId> {
    tokens
        .iter()
        .find(|(name, _)| name == TOKEN)
        .and_then(|(_, value)| value.parse().ok())
}

/// The workspaces of `snapshot` in its order, each with its chat panes (one
/// per pane, at most [`MAX_CHATS`] in all) and the tabs that hold none.
pub(crate) fn project(snapshot: &ClientShellSnapshot) -> Layout {
    let mut seen = HashSet::new();
    let chats: Vec<(String, Chat)> = snapshot
        .agents
        .iter()
        .filter_map(|agent| {
            let endpoint = endpoint(&agent.tokens)?;
            seen.insert(agent.pane_id.clone()).then(|| {
                let title = [&agent.title, &agent.terminal_title_stripped, &agent.name]
                    .into_iter()
                    .flatten()
                    .find(|title| !title.trim().is_empty())
                    .cloned()
                    .unwrap_or_else(|| tab_label(snapshot, &agent.tab_id));
                (
                    agent.workspace_id.clone(),
                    Chat {
                        pane: agent.pane_id.clone(),
                        tab: agent.tab_id.clone(),
                        endpoint,
                        agent: agent.agent.clone().or_else(|| agent.display_agent.clone()),
                        title,
                        status: agent.agent_status,
                    },
                )
            })
        })
        .take(MAX_CHATS)
        .collect();
    let workspaces = snapshot
        .workspaces
        .iter()
        .map(|workspace| {
            let id = &workspace.workspace_id;
            let chats: Vec<Chat> = chats
                .iter()
                .filter(|(owner, _)| owner == id)
                .map(|(_, chat)| chat.clone())
                .collect();
            let terminals = snapshot
                .tabs
                .iter()
                .filter(|tab| &tab.workspace_id == id)
                .filter(|tab| !chats.iter().any(|chat| chat.tab == tab.tab_id))
                .filter_map(|tab| {
                    let panes: Vec<TerminalPane> = snapshot
                        .panes
                        .iter()
                        .filter(|pane| pane.tab_id == tab.tab_id)
                        .map(|pane| TerminalPane {
                            pane: pane.pane_id.clone(),
                            label: pane.label.clone().filter(|label| !label.trim().is_empty()),
                            cwd: pane.foreground_cwd.clone().or_else(|| pane.cwd.clone()),
                            focused: pane.focused,
                        })
                        .collect();
                    let terminal = Terminal {
                        tab: tab.tab_id.clone(),
                        label: tab.label.clone(),
                        cwd: None,
                        zoomed: tab.zoomed,
                        panes,
                    };
                    let cwd = terminal.first_pane()?.cwd.clone();
                    Some(Terminal { cwd, ..terminal })
                })
                .collect();
            Workspace {
                id: id.clone(),
                label: workspace.label.clone(),
                branch: workspace.branch.clone(),
                cwd: workspace.new_workspace_cwd.clone(),
                chats,
                terminals,
            }
        })
        .collect();
    Layout {
        connected: true,
        boot: snapshot.boot_id.clone(),
        workspaces,
    }
}

fn tab_label(snapshot: &ClientShellSnapshot, tab: &str) -> String {
    snapshot
        .tabs
        .iter()
        .find(|candidate| candidate.tab_id == tab)
        .map_or_else(|| "Chat".to_owned(), |tab| tab.label.clone())
}
