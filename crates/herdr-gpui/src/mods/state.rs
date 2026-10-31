//! The state mods read, built from what the window already holds: each
//! listed host's snapshot. Usage and the pull request are not offered yet,
//! so those topics read as empty.

use crate::{HerdrWindow, sidebar::agent_name, state::ConnectionStatus};
use herdr_client::protocol::AgentStatus as WireStatus;
use herdr_mods::{Agent, AgentStatus, Focus, Host, State, Tab, Workspace};

impl HerdrWindow {
    pub(crate) fn mods_state(&self) -> State {
        let mut state = State::default();
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            if !endpoint.enabled {
                continue;
            }
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &endpoint.live
            };
            state.hosts.push(Host {
                id: endpoint.id.clone(),
                name: endpoint.label.clone(),
                connected: matches!(live.status, ConnectionStatus::Connected),
            });
            let Some(snapshot) = live.snapshot.as_deref() else {
                continue;
            };
            for workspace in &snapshot.workspaces {
                state.workspaces.push(Workspace {
                    id: workspace.workspace_id.clone(),
                    host: endpoint.id.clone(),
                    name: workspace.label.clone(),
                    focused: workspace.focused,
                    tabs: snapshot
                        .tabs
                        .iter()
                        .filter(|tab| tab.workspace_id == workspace.workspace_id)
                        .map(|tab| Tab {
                            id: tab.tab_id.clone(),
                            name: tab.label.clone(),
                            focused: tab.focused,
                        })
                        .collect(),
                });
            }
            state
                .agents
                .extend(snapshot.agents.iter().map(|agent| Agent {
                    id: agent.pane_id.clone(),
                    host: endpoint.id.clone(),
                    workspace: agent.workspace_id.clone(),
                    name: agent_name(agent).to_owned(),
                    status: status(agent.agent_status),
                }));
            if index == self.selected_endpoint {
                let workspace = snapshot.focused_workspace_id.as_deref().and_then(|id| {
                    snapshot
                        .workspaces
                        .iter()
                        .find(|workspace| workspace.workspace_id == id)
                });
                state.focus = Some(Focus {
                    host: endpoint.id.clone(),
                    workspace: snapshot.focused_workspace_id.clone(),
                    tab: snapshot.focused_tab_id.clone(),
                    name: workspace.map(|workspace| workspace.label.clone()),
                    path: workspace
                        .map(|workspace| workspace.new_workspace_cwd.clone())
                        .filter(|path| !path.is_empty()),
                });
            }
        }
        state
    }
}

fn status(status: WireStatus) -> AgentStatus {
    match status {
        WireStatus::Working => AgentStatus::Working,
        WireStatus::Blocked => AgentStatus::Blocked,
        WireStatus::Done => AgentStatus::Done,
        WireStatus::Idle | WireStatus::Unknown => AgentStatus::Idle,
    }
}
