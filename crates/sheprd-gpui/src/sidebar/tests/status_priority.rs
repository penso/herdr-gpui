use super::*;
use crate::{
    config::DEFAULT_STATUS_PRIORITY,
    preferences::AgentSort,
    sidebar::{
        agents::{sorted_agents, status_priority},
        search::{Device, Kind, search},
        workspaces::{
            displayed_sidebar_workspace_status, displayed_workspace_status, rail_workspace_status,
            workspace_status,
        },
    },
};
use std::collections::HashSet;

fn custom_priority() -> Vec<AgentStatus> {
    [
        AgentStatus::Blocked,
        AgentStatus::Working,
        AgentStatus::Done,
        AgentStatus::Idle,
    ]
    .to_vec()
}

fn mixed_snapshot() -> ClientShellSnapshot {
    let mut snapshot = layout_tests::snapshot(1);
    snapshot.workspaces[0].agent_status = AgentStatus::Done;
    snapshot.agents[0].workspace_id = "w0".into();
    snapshot.agents[0].agent_status = AgentStatus::Done;
    snapshot.agents[1].agent_status = AgentStatus::Blocked;
    let mut working = snapshot.agents[0].clone();
    working.pane_id = "working".into();
    working.agent_status = AgentStatus::Working;
    snapshot.agents.push(working);
    snapshot
}

#[test]
fn default_and_custom_orders_rank_agents_and_unknown_is_last() {
    let default = &DEFAULT_STATUS_PRIORITY;
    let custom = custom_priority();
    assert_eq!(status_priority(AgentStatus::Blocked, default), 4);
    assert_eq!(status_priority(AgentStatus::Done, default), 3);
    assert_eq!(status_priority(AgentStatus::Working, default), 2);
    assert_eq!(status_priority(AgentStatus::Unknown, &custom), 0);

    let snapshot = mixed_snapshot();
    assert_eq!(
        sorted_agents(&snapshot, AgentSort::Priority, default)
            .into_iter()
            .map(|agent| agent.agent_status)
            .collect::<Vec<_>>(),
        [
            AgentStatus::Blocked,
            AgentStatus::Done,
            AgentStatus::Working
        ]
    );
    assert_eq!(
        sorted_agents(&snapshot, AgentSort::Priority, &custom)
            .into_iter()
            .map(|agent| agent.agent_status)
            .collect::<Vec<_>>(),
        [
            AgentStatus::Blocked,
            AgentStatus::Working,
            AgentStatus::Done
        ]
    );
}

#[test]
fn default_workspace_status_matches_daemon_and_custom_uses_matching_agents() {
    let snapshot = mixed_snapshot();
    let workspace = &snapshot.workspaces[0];
    assert_eq!(
        workspace_status(workspace, &snapshot.agents, &DEFAULT_STATUS_PRIORITY),
        AgentStatus::Done
    );
    assert_eq!(
        workspace_status(workspace, &snapshot.agents, &custom_priority()),
        AgentStatus::Working
    );
    assert_eq!(
        workspace_status(workspace, &[], &custom_priority()),
        AgentStatus::Done,
        "no matching agent keeps the daemon status"
    );
    assert_eq!(
        workspace_status(
            workspace,
            std::slice::from_ref(&snapshot.agents[1]),
            &custom_priority(),
        ),
        AgentStatus::Done,
        "unrelated agents do not replace the daemon fallback"
    );
}

#[test]
fn collapsed_workspace_and_compact_rail_roll_up_child_agents_by_priority() {
    let mut snapshot = layout_tests::snapshot(6);
    for workspace in &mut snapshot.workspaces[3..6] {
        workspace.agent_status = AgentStatus::Done;
    }
    let child_id = snapshot.workspaces[4].workspace_id.clone();
    snapshot.agents[0].workspace_id = child_id;
    snapshot.agents[0].agent_status = AgentStatus::Working;
    snapshot.agents[1].agent_status = AgentStatus::Blocked;
    let collapsed = HashSet::from([layout_tests::REPO_KEY.to_owned()]);
    let parent = &snapshot.workspaces[3];
    let priority = custom_priority();
    assert_eq!(
        displayed_workspace_status(
            &snapshot.workspaces,
            &snapshot.agents,
            parent,
            &collapsed,
            &priority,
        ),
        AgentStatus::Working
    );
    assert_eq!(
        displayed_sidebar_workspace_status(
            &snapshot.workspaces,
            &snapshot.agents,
            parent,
            &collapsed,
            true,
            &priority,
        ),
        AgentStatus::Working
    );
    assert_eq!(
        rail_workspace_status(
            &snapshot.workspaces,
            &snapshot.agents,
            parent,
            &collapsed,
            &priority,
        ),
        AgentStatus::Working
    );
    assert_eq!(
        rail_workspace_status(
            &snapshot.workspaces,
            &snapshot.agents,
            parent,
            &collapsed,
            &DEFAULT_STATUS_PRIORITY,
        ),
        AgentStatus::Done
    );
}

#[test]
fn default_order_keeps_the_base_inline_false_collapsed_path() {
    let mut snapshot = layout_tests::snapshot(6);
    snapshot.workspaces[3].agent_status = AgentStatus::Idle;
    snapshot.workspaces[4].agent_status = AgentStatus::Blocked;
    let collapsed = HashSet::from([layout_tests::REPO_KEY.to_owned()]);
    let parent = &snapshot.workspaces[3];
    assert_eq!(
        displayed_sidebar_workspace_status(
            &snapshot.workspaces,
            &snapshot.agents,
            parent,
            &collapsed,
            false,
            &DEFAULT_STATUS_PRIORITY,
        ),
        AgentStatus::Idle
    );
}

#[test]
fn workspace_search_uses_custom_status_priority() {
    let snapshot = mixed_snapshot();
    let custom = custom_priority();
    let device = Device {
        id: "local",
        label: "MacBook Pro",
        workspaces: &snapshot.workspaces,
        agents: &snapshot.agents,
        status_priority: &custom,
        notes: None,
    };
    let hit = search("herdr", &[device])
        .into_iter()
        .find(|hit| hit.kind == Kind::Worktree)
        .unwrap();
    assert_eq!(hit.status, Some(AgentStatus::Working));
}
