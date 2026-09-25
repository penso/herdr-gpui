#![allow(clippy::unwrap_used)]

use super::{
    agents::{agent_labels, agent_row_label},
    layout_tests,
    metrics::{sidebar_width, split_fraction},
    row::{first_text, status_color},
    workspace_label,
    workspaces::workspace_entries,
};
use herdr_client::protocol::{
    AgentStatus, ClientShellAgent, ClientShellSnapshot, ClientShellWorkspace,
};

#[test]
fn hierarchy_uses_git_metadata_and_emits_each_workspace_once() {
    let mut workspaces = layout_tests::snapshot(7).workspaces;
    for workspace in &mut workspaces {
        workspace.worktree = None;
        workspace.label = "same label".into();
        workspace.branch = Some("main".into());
    }
    for (index, key, linked) in [
        (0, "/repo/.git", true),
        (2, "/repo/.git", false),
        (3, "/orphan/.git", true),
        (4, "/repo/.git", true),
        (5, "/other/.git", false),
        (6, "/orphan/.git", true),
    ] {
        workspaces[index].worktree = Some(herdr_client::protocol::ClientShellWorktree {
            key: key.into(),
            label: "same repo name".into(),
            is_linked_worktree: linked,
        });
    }
    workspaces[2].branch = Some("develop".into());
    assert_eq!(
        workspace_entries(&workspaces),
        vec![
            (2, false),
            (0, true),
            (4, true),
            (1, false),
            (3, false),
            (5, false),
            (6, false),
        ]
    );
    workspaces[2].worktree = None;
    assert_eq!(
        workspace_entries(&workspaces),
        (0..7).map(|i| (i, false)).collect::<Vec<_>>()
    );
    assert!(workspace_entries(&[]).is_empty());
}

#[test]
fn collapse_uses_repository_identity_without_mutating_selection() {
    let mut workspaces = layout_tests::snapshot(7).workspaces;
    workspaces[4].focused = true;
    let before = workspaces.clone();
    let collapsed = std::collections::HashSet::from([layout_tests::REPO_KEY.into()]);
    let entries = super::visible_workspace_entries(&workspaces, &collapsed);
    assert_eq!(
        entries.iter().map(|entry| entry.0).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 6]
    );
    assert_eq!(entries.iter().filter(|entry| entry.2.is_some()).count(), 1);
    assert_eq!(workspaces, before);
    workspaces[3].label = "renamed".into();
    assert_eq!(
        super::visible_workspace_entries(&workspaces, &collapsed).len(),
        5
    );
    workspaces.remove(5);
    workspaces.remove(4);
    assert!(
        super::visible_workspace_entries(&workspaces, &collapsed)
            .iter()
            .all(|entry| entry.2.is_none())
    );
    workspaces.remove(3);
    assert_eq!(
        super::visible_workspace_entries(&workspaces, &collapsed).len(),
        4
    );
}

#[test]
fn child_labels_follow_upstream_custom_label_and_branch_rules() {
    let mut workspace = layout_tests::snapshot(1).workspaces.remove(0);
    workspace.branch = Some("worktree/fix-sidebar".into());
    assert_eq!(workspace_label(&workspace, true), "fix-sidebar");
    assert_eq!(workspace_label(&workspace, false), "herdr");
    workspace.custom_label = true;
    assert_eq!(workspace_label(&workspace, true), "herdr");
    workspace.custom_label = false;
    workspace.branch = None;
    assert_eq!(workspace_label(&workspace, true), "herdr");
}

#[test]
fn text_fallback_skips_missing_and_blank_metadata() {
    assert_eq!(first_text([None, Some(" \t"), Some(" main ")], ""), "main");
    assert_eq!(first_text([None, Some("")], ""), "");
    assert_eq!(first_text([Some(" ")], "workspace"), "workspace");
}

#[test]
fn agent_rows_name_their_place_then_their_agent() {
    let mut snapshot = layout_tests::snapshot(1);
    let agent = &mut snapshot.agents[0];
    agent.workspace_id = "w0".into();
    agent.tab_id = "t0".into();
    agent.display_agent = Some("Claude Code".into());
    agent.name = Some("review".into());
    agent.agent = Some("claude".into());
    agent.title = Some("Fix sidebar".into());
    let agent = snapshot.agents[0].clone();
    // Host first when there is one, then the workspace, then the tab. Only
    // the workspace is primary; upstream mutes what sits around it.
    fn labels<'a>(
        snapshot: &'a ClientShellSnapshot,
        host: Option<&'a str>,
    ) -> (Vec<(&'a str, bool)>, &'a str) {
        agent_labels(&snapshot.agents[0], snapshot, host)
    }
    assert_eq!(
        labels(&snapshot, None),
        (vec![("herdr", true), ("tab 1", false)], "Claude Code")
    );
    assert_eq!(
        labels(&snapshot, Some("remote")),
        (
            vec![("remote", false), ("herdr", true), ("tab 1", false)],
            "Claude Code"
        )
    );
    // One unnamed tab is noise, so only its workspace shows.
    snapshot.tabs.retain(|tab| tab.tab_id == "t0");
    assert_eq!(labels(&snapshot, None).0, vec![("herdr", true)]);
    snapshot.tabs[0].custom_label = true;
    assert_eq!(
        labels(&snapshot, None).0,
        vec![("herdr", true), ("tab 1", false)]
    );
    // The agent name falls back through the same order as upstream.
    for (display, name, kind, title, expected) in [
        (None, Some("review"), Some("claude"), None, "review"),
        (None, None, Some("claude"), Some("Fix sidebar"), "claude"),
        (None, None, None, Some("Fix sidebar"), "Fix sidebar"),
        (None, None, None, None, "agent"),
    ] {
        snapshot.agents[0] = ClientShellAgent {
            display_agent: display.map(str::to_owned),
            name: name.map(str::to_owned),
            agent: kind.map(str::to_owned),
            title: title.map(str::to_owned),
            ..agent.clone()
        };
        assert_eq!(labels(&snapshot, None).1, expected);
    }
    // Without its workspace the agent names the row itself.
    snapshot.workspaces.clear();
    assert_eq!(labels(&snapshot, None), (vec![("agent", true)], ""));
}

#[test]
fn agent_rows_read_agent_first_then_place() {
    assert_eq!(
        agent_row_label(
            &[("remote", false), ("herdr", true), ("tab 1", false)],
            "Claude Code"
        ),
        "Claude Code \u{b7} remote \u{b7} herdr \u{b7} tab 1"
    );
    // An orphan names itself in its only segment.
    assert_eq!(agent_row_label(&[("agent", true)], ""), "agent");
}

#[test]
fn status_colors_come_from_the_theme_semantics() {
    let theme = gpui_kit::component::ThemeColor::default();
    for (status, expected) in [
        (AgentStatus::Working, theme.warning),
        (AgentStatus::Blocked, theme.danger),
        (AgentStatus::Done, theme.info),
        (AgentStatus::Idle, theme.success),
        (AgentStatus::Unknown, theme.muted_foreground),
    ] {
        assert_eq!(status_color(status, &theme), expected, "{status:?}");
    }
}

#[test]
fn status_wire_casing_round_trips() {
    let snapshot = layout_tests::snapshot(1);
    for (wire, status) in [
        ("idle", AgentStatus::Idle),
        ("working", AgentStatus::Working),
        ("blocked", AgentStatus::Blocked),
        ("done", AgentStatus::Done),
        ("unknown", AgentStatus::Unknown),
    ] {
        let mut value = serde_json::to_value(&snapshot.workspaces[0]).unwrap();
        value["agent_status"] = wire.into();
        let workspace: ClientShellWorkspace = serde_json::from_value(value).unwrap();
        assert_eq!(workspace.agent_status, status);
        let mut value = serde_json::to_value(&snapshot.agents[0]).unwrap();
        value["agent_status"] = wire.into();
        let agent: ClientShellAgent = serde_json::from_value(value).unwrap();
        assert_eq!(agent.agent_status, status);
        assert_eq!(serde_json::to_value(status).unwrap(), wire);
    }
}

#[test]
fn stored_width_and_split_are_clamped() {
    assert_eq!(sidebar_width(None, 1200.), 232.);
    assert_eq!(sidebar_width(Some(10.), 1200.), 160.);
    assert_eq!(sidebar_width(Some(900.), 1200.), 480.);
    // The terminal keeps its share of a narrow window.
    assert_eq!(sidebar_width(Some(300.), 500.), 260.);
    assert_eq!(split_fraction(None), 0.5);
    assert_eq!(split_fraction(Some(0.)), 0.1);
    assert_eq!(split_fraction(Some(2.)), 0.9);
}
