#![allow(clippy::unwrap_used)]

use super::{
    STATUS_DOT_UNKNOWN, STATUS_WIDTH, agent_name,
    agents::{agent_labels, agent_place, status_style},
    layout_tests,
    render::header,
    row::first_text,
    workspace_label,
    workspaces::workspace_entries,
};
use crate::{
    config::{FontConfig, Theme},
    contrast::Contrast,
};
use herdr_client::protocol::{
    AgentStatus, ClientShellAgent, ClientShellSnapshot, ClientShellWorkspace,
};

#[test]
fn section_headings_use_the_configured_sidebar_font_size() {
    use gpui::{Styled, px};
    for size in [12., 16., 20.] {
        let font = FontConfig {
            family: "Menlo".into(),
            size,
            fallbacks: None,
        };
        for label in ["spaces", "agents"] {
            let mut heading = header(
                label,
                &font,
                &Theme::default(),
                super::layout::for_mode(Default::default()),
                0.,
            );
            assert_eq!(heading.text_style().font_size, Some(px(size).into()));
        }
    }
}

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
        let agent = &snapshot.agents[0];
        agent_labels(agent_name(agent), agent_place(agent, snapshot), host)
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
fn rows_weight_and_dim_their_text_like_upstream() {
    use super::row::{RowKind, row_text};
    use gpui::FontWeight;
    let theme = Theme::default();
    // Agents stay bold whether or not they are the current row; a workspace
    // earns bold only while focused, and hands its branch the accent then.
    for (kind, focused, weight, name, detail) in [
        (
            RowKind::Agent(crate::icons::AgentIcon::Generic),
            false,
            FontWeight::BOLD,
            theme.subtext(),
            theme.muted,
        ),
        (
            RowKind::Agent(crate::icons::AgentIcon::Generic),
            true,
            FontWeight::BOLD,
            theme.foreground,
            theme.muted,
        ),
        (
            RowKind::Workspace,
            false,
            FontWeight::NORMAL,
            theme.subtext(),
            theme.muted,
        ),
        (
            RowKind::Workspace,
            true,
            FontWeight::BOLD,
            theme.foreground,
            theme.primary(),
        ),
    ] {
        assert_eq!(row_text(kind, focused, &theme), (name, weight, detail));
    }
    // Subtext sits between the muted detail and the focused name.
    let brightness = |color: u32| (color >> 16) + ((color >> 8) & 255) + (color & 255);
    assert!(brightness(theme.muted) < brightness(theme.subtext()));
    assert!(brightness(theme.subtext()) < brightness(theme.foreground));
}

const STATUSES: [AgentStatus; 5] = [
    AgentStatus::Working,
    AgentStatus::Blocked,
    AgentStatus::Done,
    AgentStatus::Idle,
    AgentStatus::Unknown,
];

#[test]
fn status_colors_keep_upstream_literals_where_they_already_read() {
    // Upstream's default palette (Catppuccin Mocha), which its status dots use
    // whatever terminal colors are loaded.
    for (status, color) in [
        (AgentStatus::Working, 0xf9e2af),
        (AgentStatus::Blocked, 0xf38ba8),
        (AgentStatus::Done, 0x94e2d5),
        (AgentStatus::Idle, 0xa6e3a1),
    ] {
        for name in ["Default", "Nord", "Dracula", "Catppuccin Mocha"] {
            let mut theme = Theme::builtin(name).unwrap();
            assert_eq!(status_style(status, &theme).2, color, "{name}");
            // ANSI slots must not change the meaning of a status color.
            theme.palette.fill(0x123456);
            assert_eq!(status_style(status, &theme).2, color, "{name}");
        }
    }
    // Upstream's Unknown grey falls just short of 3:1 on a selected row, even
    // in Mocha itself, so it is lifted by only a few steps and stays grey.
    let mocha = Theme::builtin("Catppuccin Mocha").unwrap();
    let unknown = status_style(AgentStatus::Unknown, &mocha).2;
    let channels = |color: u32| [16, 8, 0].map(|shift| ((color >> shift) & 255) as i32);
    for (lifted, upstream) in channels(unknown).into_iter().zip(channels(0x6c7086)) {
        assert!((0..=16).contains(&(lifted - upstream)), "{unknown:06x}");
    }
}

#[test]
fn status_colors_reach_the_contrast_setting_on_every_builtin_theme() {
    for name in Theme::BUILTIN_NAMES {
        for contrast in [Contrast::Standard, Contrast::High] {
            let theme = Theme::builtin(name).unwrap().with_contrast(contrast);
            for status in STATUSES {
                let color = status_style(status, &theme).2;
                for background in [theme.background, theme.surface, theme.active] {
                    let ratio = crate::contrast::ratio(color, background);
                    assert!(
                        ratio >= contrast.mark_ratio(),
                        "{name} {contrast:?} {status:?} on {background:06x}: {ratio}"
                    );
                }
            }
        }
    }
    // Light chrome darkens the pastels rather than keeping upstream's literals.
    let latte = Theme::builtin("Catppuccin Latte").unwrap();
    let mocha = Theme::builtin("Catppuccin Mocha").unwrap();
    for status in STATUSES {
        assert_ne!(
            status_style(status, &latte).2,
            status_style(status, &mocha).2
        );
    }
}

#[test]
fn status_shapes_match_upstream_dots_and_wire_casing() {
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
        let theme = Theme::default();
        let (diameter, filled, color) = status_style(status, &theme);
        assert_eq!(
            color,
            theme.ink(match status {
                AgentStatus::Working => 0xf9e2af,
                AgentStatus::Blocked => 0xf38ba8,
                AgentStatus::Done => 0x94e2d5,
                AgentStatus::Idle => 0xa6e3a1,
                AgentStatus::Unknown => 0x6c7086,
            })
        );
        assert_eq!(filled, status != AgentStatus::Idle);
        assert_eq!(
            diameter,
            if status == AgentStatus::Unknown {
                STATUS_DOT_UNKNOWN
            } else {
                STATUS_WIDTH
            }
        );
    }
}

#[test]
fn cells_hand_their_state_and_data_to_the_layout() {
    use super::{
        cell::{AgentRow, Cell, RowContext, RowData, RowLayout, RowState, WorkspaceRow},
        layout::for_mode,
        row::{RowIcon, RowTree},
    };
    use gpui::{Div, div};
    use std::cell::RefCell;

    /// Records what each call was given instead of drawing it.
    #[derive(Default)]
    struct Recorder(RefCell<Vec<(String, RowState)>>);

    impl RowLayout for Recorder {
        fn workspace(&self, row: WorkspaceRow<'_>, state: RowState, _: &RowContext<'_>) -> Div {
            self.0.borrow_mut().push((row.label.to_owned(), state));
            div()
        }
        fn agent(&self, row: AgentRow<'_>, state: RowState, _: &RowContext<'_>) -> Div {
            self.0.borrow_mut().push((row.key, state));
            div()
        }
    }

    let snapshot = layout_tests::snapshot(1);
    let (font, theme) = (crate::config::Config::default().sidebar, Theme::default());
    let cx = RowContext {
        font: &font,
        theme: &theme,
        look: for_mode(Default::default()),
        width: 232.,
        host: None,
    };
    let recorder = Recorder::default();
    let workspace = || {
        RowData::Workspace(WorkspaceRow {
            workspace: &snapshot.workspaces[0],
            label: "herdr",
            tree: RowTree::None,
            icon: RowIcon::None,
            fold: None,
            grouped: false,
            badge: None,
            removing: false,
        })
    };
    let _ = Cell::new(&recorder, workspace(), &cx).row();
    let _ = Cell::new(&recorder, workspace(), &cx).selected(true).row();
    let _ = Cell::new(
        &recorder,
        RowData::Agent(AgentRow {
            key: "agent-p0".into(),
            name: "Claude Code",
            icon: crate::icons::AgentIcon::Generic,
            status: AgentStatus::Working,
            place: None,
            status_text: None,
        }),
        &cx,
    )
    .highlighted(true)
    .row();
    let state = |selected, highlighted| RowState {
        selected,
        highlighted,
        ..RowState::default()
    };
    assert_eq!(
        recorder.0.into_inner(),
        vec![
            ("herdr".into(), state(false, false)),
            ("herdr".into(), state(true, false)),
            ("agent-p0".into(), state(false, true)),
        ]
    );
}
