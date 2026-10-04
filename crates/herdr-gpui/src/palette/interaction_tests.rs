#![allow(clippy::unwrap_used)]

use super::*;
use core::prelude::v1::test;
use gpui::TestAppContext;
use std::sync::Arc;

#[gpui::test]
fn one_palette_combines_navigation_actions_commands_and_projects(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot
                .commands
                .push(herdr_client::protocol::ClientShellCommand {
                    command_id: "build".into(),
                    action: ClientShellCommandAction::Shell,
                    description: Some("Build project".into()),
                    binding_label: String::new(),
                    binding_labels: Vec::new(),
                });
            view.open_palette(Filter::All, window, cx);
            let mut palette = view.menu.palette.take().unwrap();
            palette.projects.projects.push(projects::Project {
                path: "/projects/herdr".into(),
                label: "herdr".into(),
            });
            view.prepare_palette_entries(&mut palette);
            for filter in Filter::ALL {
                palette.filter = filter;
                palette.filter("");
                assert!(!palette.filtered.is_empty());
                assert!(
                    palette
                        .filtered
                        .iter()
                        .all(|index| filter.accepts(&palette.entries[*index].action))
                );
            }
            palette.filter = Filter::All;
            palette.filter("build");
            assert!(matches!(
                palette.entries[palette.filtered[0]].action,
                Action::Configured(..)
            ));
            palette.filter("projects herdr");
            assert_eq!(palette.filtered.len(), 1);
            assert!(matches!(
                palette.entries[palette.filtered[0]].action,
                Action::Project(_)
            ));
            view.menu.palette = Some(palette);
        })
    });
}

#[gpui::test]
fn metadata_refresh_preserves_selection_and_command_invocation_context(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_palette(Filter::All, window, cx);
            let palette = view.menu.palette.as_mut().unwrap();
            palette.filter("Settings");
            let selected = palette.selected_identity();
            let captured = palette.target.as_ref().unwrap().workspace.clone();
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_workspace_id = None;
            view.refresh_palette(window, cx);
            let palette = view.menu.palette.as_ref().unwrap();
            assert_eq!(palette.selected_identity(), selected);
            assert_eq!(palette.target.as_ref().unwrap().workspace, captured);
            assert_eq!(palette.query, "Settings");
        })
    });
}

#[gpui::test]
fn project_loading_is_incremental_and_config_changes_rescan_without_losing_query(
    cx: &mut TestAppContext,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("herdr")).unwrap();
    let other = tempfile::tempdir().unwrap();
    std::fs::create_dir(other.path().join("herdr-next")).unwrap();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.palette.project_roots = vec![root.path().to_string_lossy().into_owned()];
            view.open_palette(Filter::All, window, cx);
            let palette = view.menu.palette.as_mut().unwrap();
            assert!(palette.loading_projects);
            assert!(
                palette
                    .entries
                    .iter()
                    .any(|entry| matches!(entry.action, Action::Native(_)))
            );
            palette.filter("herdr");
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let palette = view.menu.palette.as_ref().unwrap();
        assert!(!palette.loading_projects);
        assert_eq!(palette.projects.projects.len(), 1);
        assert_eq!(palette.projects.projects[0].label, "herdr");
        assert_eq!(palette.query, "herdr");
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.palette.project_roots = vec![other.path().to_string_lossy().into_owned()];
            view.refresh_palette(window, cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let palette = view.menu.palette.as_ref().unwrap();
        assert_eq!(palette.projects.projects[0].label, "herdr-next");
        assert_eq!(palette.query, "herdr");
    });
}

#[gpui::test]
fn a_closed_palettes_scan_cannot_fill_a_reopened_palette(cx: &mut TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("old-project")).unwrap();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.palette.project_roots = vec![root.path().to_string_lossy().into_owned()];
            view.open_palette(Filter::All, window, cx);
            view.dismiss_menu(window, cx);
            view.config.palette.project_roots.clear();
            view.open_palette(Filter::Commands, window, cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let palette = view.menu.palette.as_ref().unwrap();
        assert!(palette.projects.projects.is_empty());
        assert!(!palette.loading_projects);
        assert_eq!(palette.filter, Filter::Commands);
    });
}

#[gpui::test]
fn native_search_input_owns_text_and_filter_navigation(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_palette(Filter::All, window, cx));
        window.draw(cx).clear(cx);
    });
    cx.simulate_input("Settings");
    cx.simulate_keystrokes("tab");
    view.read_with(cx, |view, cx| {
        let palette = view.menu.palette.as_ref().unwrap();
        assert_eq!(palette.search.read(cx).text(), "Settings");
        assert_eq!(palette.filter, Filter::Navigation);
        assert!(view.marked.is_empty());
    });
    cx.simulate_keystrokes("shift-tab");
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.menu.palette.is_none()));
}

#[gpui::test]
fn narrow_layout_keeps_filters_and_long_result_rows_inside_the_window(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    for (width, height) in [(360., 420.), (900., 700.)] {
        cx.simulate_resize(size(px(width), px(height)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                Arc::make_mut(view.live.snapshot.as_mut().unwrap()).workspaces[0].label =
                    "a very long workspace label ".repeat(20);
                view.open_palette(Filter::Navigation, window, cx);
            });
            window.draw(cx).clear(cx);
        });
        for selector in [
            "palette-row-0",
            "palette-filter-projects",
            "palette-agent-filter-done",
            "palette-status",
        ] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(
                bounds.left() >= px(0.) && bounds.right() <= px(width),
                "{selector}: {bounds:?}"
            );
            assert!(
                bounds.top() >= px(0.) && bounds.bottom() <= px(height),
                "{selector}: {bounds:?}"
            );
        }
    }
}

#[gpui::test]
fn composition_does_not_activate_or_dismiss_the_palette(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_palette(Filter::All, window, cx);
            let search = view.menu.palette.as_ref().unwrap().search.clone();
            search.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(None, "設定", Some(2..2), window, cx)
            });
            for key in ["enter", "escape", "tab", "alt-b"] {
                view.palette_key(
                    &KeyDownEvent {
                        keystroke: Keystroke::parse(key).unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    },
                    window,
                    cx,
                );
                assert!(view.menu.palette.is_some());
                let palette = view.menu.palette.as_ref().unwrap();
                assert_eq!(palette.filter, Filter::All);
                assert_eq!(palette.agent_filter, AgentFilter::All);
            }
            assert!(view.marked.is_empty());
            search.update(cx, |input, cx| input.unmark_text(window, cx));
            view.palette_key(
                &KeyDownEvent {
                    keystroke: Keystroke::parse("escape").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            );
            assert!(view.menu.palette.is_none());
        })
    });
}

/// Go To rows of `palette` in list order, named by their destination.
fn go_to_rows(palette: &Palette) -> Vec<String> {
    palette
        .filtered
        .iter()
        .map(|index| match &palette.entries[*index].action {
            Action::Go { target, .. } => match target {
                NavigationTarget::Workspace(id)
                | NavigationTarget::Tab(id)
                | NavigationTarget::Pane(id) => id.clone(),
            },
            _ => "other".into(),
        })
        .collect()
}

/// w1 holds a blocked agent and a plain terminal, w2 a working agent, and
/// w3 nothing at all.
fn status_fixture(window: &mut Window, cx: &mut Context<HerdrWindow>) -> HerdrWindow {
    let mut view = fixture_window(window, cx);
    let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    let mut terminal = snapshot.panes[0].clone();
    terminal.pane_id = "w1:p2".into();
    terminal.label = Some("logs".into());
    snapshot.panes.push(terminal);
    for (number, id) in [(2, "w2"), (3, "w3")] {
        let mut workspace = snapshot.workspaces[0].clone();
        workspace.workspace_id = id.into();
        workspace.number = number;
        workspace.label = format!("project {id}");
        workspace.branch = None;
        snapshot.workspaces.push(workspace);
    }
    let mut tab = snapshot.tabs[0].clone();
    tab.tab_id = "w2:t1".into();
    tab.workspace_id = "w2".into();
    snapshot.tabs.push(tab);
    let mut pane = snapshot.panes[0].clone();
    pane.pane_id = "w2:p1".into();
    pane.workspace_id = "w2".into();
    pane.tab_id = "w2:t1".into();
    snapshot.panes.push(pane);
    let mut agent = snapshot.agents[0].clone();
    agent.pane_id = "w2:p1".into();
    agent.workspace_id = "w2".into();
    agent.tab_id = "w2:t1".into();
    agent.display_agent = Some("Codex".into());
    agent.agent_status = AgentStatus::Working;
    agent.state_labels.clear();
    snapshot.agents.push(agent);
    view.endpoints[0].live = view.live.clone();
    view
}

#[gpui::test]
fn alt_letters_filter_go_to_by_daemon_agent_status_without_typing(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(status_fixture);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_palette(Filter::All, window, cx));
        window.draw(cx).clear(cx);
    });
    let state = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let palette = view.menu.palette.as_ref().unwrap();
            (
                palette.filter,
                palette.agent_filter,
                palette.search.read(cx).text().to_owned(),
                go_to_rows(palette),
            )
        })
    };
    let all = ["w1", "w1:p1", "w1:p2", "w2", "w2:p1", "w3"].map(String::from);
    for (keys, agent_filter, rows) in [
        ("alt-w", AgentFilter::Working, &["w2", "w2:p1"][..]),
        ("alt-b", AgentFilter::Blocked, &["w1", "w1:p1"]),
        ("alt-i", AgentFilter::Idle, &[]),
        ("alt-d", AgentFilter::Done, &[]),
    ] {
        cx.simulate_keystrokes(keys);
        assert_eq!(
            state(cx),
            (
                Filter::Navigation,
                agent_filter,
                String::new(),
                rows.iter().map(|row| row.to_string()).collect()
            ),
            "{keys}"
        );
    }
    cx.simulate_keystrokes("alt-a");
    assert_eq!(
        state(cx),
        (
            Filter::Navigation,
            AgentFilter::All,
            String::new(),
            all.to_vec()
        )
    );

    // The search still narrows within a status, and a workspace never stands
    // alone as the heading of no agents.
    cx.simulate_keystrokes("alt-b");
    cx.simulate_input("claude");
    assert_eq!(state(cx).3, ["w1", "w1:p1"]);
    cx.simulate_input(" zzz");
    assert!(state(cx).3.is_empty());
    view.update(cx, |view, cx| {
        let search = view.menu.palette.as_ref().unwrap().search.clone();
        search.update(cx, |input, cx| input.clear(cx));
    });

    // Other filters ignore the status, and Navigation keeps it.
    cx.simulate_keystrokes("tab");
    let (filter, agent_filter, _, rows) = state(cx);
    assert_eq!(
        (filter, agent_filter),
        (Filter::Commands, AgentFilter::Blocked)
    );
    assert!(rows.iter().all(|row| row == "other") && !rows.is_empty());
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(state(cx).3, ["w1", "w1:p1"]);

    // Plain letters are search text, not status shortcuts.
    cx.simulate_input("b");
    let (_, agent_filter, query, _) = state(cx);
    assert_eq!((agent_filter, query.as_str()), (AgentFilter::Blocked, "b"));
}

#[gpui::test]
fn status_chips_show_only_in_navigation_and_choose_a_status(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(status_fixture);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_palette(Filter::Commands, window, cx)
        });
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("palette-agent-filter-working").is_none());
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            let palette = view.menu.palette.as_mut().unwrap();
            palette.filter = Filter::Navigation;
            palette.refilter(None);
        });
        window.draw(cx).clear(cx);
    });
    let chip = cx.debug_bounds("palette-agent-filter-working").unwrap();
    cx.simulate_click(chip.center(), Modifiers::none());
    view.read_with(cx, |view, _| {
        let palette = view.menu.palette.as_ref().unwrap();
        assert_eq!(palette.agent_filter, AgentFilter::Working);
        assert_eq!(go_to_rows(palette), ["w2", "w2:p1"]);
    });
    for selector in [
        "palette-agent-filter-all-agents",
        "palette-agent-filter-blocked",
        "palette-agent-filter-idle",
        "palette-agent-filter-done",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
}
