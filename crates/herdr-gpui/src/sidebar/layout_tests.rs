//! Window fixtures shared by the headless and native full-window tests, and
//! the sidebar's own behavior tests.
#![allow(clippy::unwrap_used)]
use herdr_client::protocol::*;
#[cfg(test)]
use {
    crate::{HerdrWindow, LiveState, WheelAccumulator},
    gpui_kit::{
        App, ArenaClearNeeded, Bounds, Context, Entity, Modifiers, Task, Window, point, prelude::*,
        px, size,
    },
    herdr_client::{ConnectOptions, ConnectTarget},
    std::sync::Arc,
};

#[cfg(test)]
struct SidebarFixture(Entity<HerdrWindow>);

#[cfg(test)]
impl Render for SidebarFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.clone()
    }
}

pub(crate) const REPO_KEY: &str = if cfg!(windows) {
    "C:/fixture/agent-launcher/.git"
} else {
    "/fixture/agent-launcher/.git"
};

pub(crate) fn snapshot(workspace_count: usize) -> ClientShellSnapshot {
    serde_json::from_value(serde_json::json!({
        "boot_id": "layout-test", "revision": 1,
        "update_install_command": "", "latest_release_notes_available": false,
        "integration_updates_available": false, "worktree_directory": "",
        "tab_bar_right": [], "tab_bar_right_separator": "", "agent_order": [],
        // A workspace always has at least one tab; two here so the agents panel
        // has a tab label to show, as it does against a live daemon.
        "tabs": (0..2).map(|i| serde_json::json!({
            "tab_id": format!("t{i}"), "workspace_id": "w0", "number": i + 1,
            "label": format!("tab {}", i + 1), "custom_label": false,
            "zoomed": false, "focused": i == 0, "agent_status": "working"
        })).collect::<Vec<_>>(),
        "panes": [], "commands": [],
        "workspaces": (0..workspace_count).map(|i| serde_json::json!({
            "workspace_id": format!("w{i}"), "active_tab_id": "t0", "new_workspace_cwd": "/tmp",
            "number": i + 1,
            "label": match i { 0 => "herdr", 1 => "herdr-gpui-sidebar-rendering-regression-investigation", 3..=5 => "agent-launcher", _ => "another workspace" },
            "custom_label": false,
            "branch": match i { 0 => "main", 2 => "1256789", 3 => "develop", 4 => "worktree/sidebar-child", 5 => "worktree/sidebar-child-with-a-long-readable-branch-name", _ => "fix/sidebar-label-width-and-overflow-regression" },
            "worktree": if (3..=5).contains(&i) { serde_json::json!({
                "key": REPO_KEY, "label": "agent-launcher", "is_linked_worktree": i != 3
            }) } else { serde_json::Value::Null },
            "tokens": [], "focused": i == 0, "agent_status": "working"
        })).collect::<Vec<_>>(),
        "agents": (["review", "Investigate sidebar rendering and verify long agent labels"].into_iter().enumerate().map(|(i, name)| serde_json::json!({
            "pane_id": format!("p{i}"), "workspace_id": if i == 0 { "w0" } else { "w1" },
            "tab_id": if i == 0 { "t0" } else { "none" },
            "name": name, "display_agent": if i == 0 { "Claude Code" } else { "agent" }, "agent": "claude",
            "agent_status": "working", "state_change_seq": 0, "state_labels": [],
            "tokens": [], "focused": false
        })).collect::<Vec<_>>())
    })).unwrap()
}

#[gpui_kit::test]
fn terminal_redraws_reuse_the_cached_sidebar(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    let renders = |cx: &mut gpui_kit::VisualTestContext| {
        cx.update(|_, cx| view.read(cx).sidebar_view.read(cx).renders)
    };
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let first = renders(cx);
    assert!(first > 0);

    // Terminal output: the window redraws, the rows do not rebuild.
    for _ in 0..3 {
        view.update(cx, |view, cx| view.redraw_terminal(cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    assert_eq!(renders(cx), first);

    // Anything else notifies the window, which rebuilds the rows as before.
    view.update(cx, |view, cx| {
        view.sidebar_width = Some(200.);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(renders(cx), first + 1);
    // Debug bounds are only recorded when painted, so read them from a full frame.
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(renders(cx), first + 2);
    assert_eq!(
        cx.debug_bounds("sidebar").map(|b| b.size.width),
        Some(px(200.))
    );

    // A hidden sidebar is not built, even for a full frame.
    view.update(cx, |view, cx| {
        view.sidebar_visible = false;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(renders(cx), first + 2);
}

/// A frame that renders every view. The sidebar is a cached view, which GPUI
/// replays without recording debug bounds; these tests measure layout, so each
/// of their frames is a full one, as every frame was before the cache.
#[cfg(test)]
pub(crate) fn full_draw(window: &mut Window, cx: &mut App) -> ArenaClearNeeded {
    window.refresh();
    window.draw(cx)
}

#[cfg(test)]
pub(crate) fn fixture_window(window: &mut Window, cx: &mut Context<HerdrWindow>) -> HerdrWindow {
    // Fixtures opened without `test_support::add_window_view` still render
    // kit components, which read the kit's theme global.
    crate::test_support::init_kit(cx);
    HerdrWindow {
        sound: Default::default(),
        updater: crate::updater::Updater::default(),
        update_preview: None,
        removal: None,
        selection: None,
        flash: None,
        configured_terminal_size: crate::config::Config::default().terminal.size,
        // Keep the original geometry fixture explicit; density-switching tests
        // above exercise all three modes independently of the default.
        config: crate::config::Config {
            layout: crate::config::Layout {
                mode: crate::config::LayoutMode::from(crate::config::Density::Comfortable),
                ..Default::default()
            },
            ..Default::default()
        },
        theme: Default::default(),
        config_load: None,
        config_watch: None,
        config_load_revision: 0,
        git: Default::default(),
        sidebar_visible: true,
        device_filter: None,
        endpoints: vec![crate::endpoint::Endpoint::new(
            crate::endpoint::LOCAL.into(),
            "Local".into(),
            ConnectTarget::Socket("/unused-layout-test.sock".into()),
            true,
        )],
        selected_endpoint: 0,
        selection_epoch: 0,
        catalog: crate::endpoint::Catalog::new(&ConnectTarget::Socket(
            "/unused-layout-test.sock".into(),
        )),
        activation_deadline: None,
        pending_navigation: None,
        pending_toast: None,
        toasts_hidden: false,
        toast_cards: Vec::new(),
        pending_releases: Vec::new(),
        selected_generation: 0,
        live: {
            let mut live = LiveState::default();
            live.snapshot = Some(Arc::new(snapshot(40)));
            live
        },
        focus: cx.focus_handle(),
        options: ConnectOptions::default(),
        last_queued_options: None,
        pending_resize: None,
        active: false,
        sent_focus: None,
        bounds: Bounds::default(),
        title: crate::WINDOW_TITLE.to_owned(),
        cell_width: 9.,
        hovered_terminal_link: false,
        pressed_terminal_link: None,
        terminal_mouse: None,
        scrollbar_drag: None,
        split_drag: None,
        split_cursor: None,
        pending_images: Vec::new(),
        file_transfer: None,
        presentation: Default::default(),
        painter: Default::default(),
        marked: String::new(),
        hover: None,
        hover_menu: None,
        local_error: None,
        menu: crate::menu::MenuState::new(cx),
        install_warning_shown: false,
        collapsed_repos: Default::default(),
        wheel: WheelAccumulator::default(),
        sidebar_width: None,
        workspace_drag: None,
        sidebar_panels: crate::sidebar::Panels::new(cx),
        sidebar_split: None,
        sidebar_split_modified: false,
        sidebar_preferences: None,
        sidebar_modified: false,
        agent_sort: Default::default(),
        agent_sort_modified: false,
        avatars: None,
        #[cfg(feature = "integration-test")]
        input_probe: crate::smoke::InputProbe::default(),
        sidebar_scroll: Default::default(),
        sidebar_revealed: Default::default(),
        _poll: Task::ready(()),
        _activation: cx.observe_window_activation(window, |_, _, _| {}),
        sidebar_view: {
            let weak = cx.weak_entity();
            cx.new(|_| crate::sidebar::SidebarView::new(weak))
        },
        surface_signal: cx.new(|_| crate::window::SurfaceSignal),
        _sidebar_invalidation: HerdrWindow::invalidate_sidebar(cx),
    }
}

#[gpui_kit::test]
fn the_sidebar_follows_the_selection_without_undoing_manual_scrolling(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    // The window paints while the connection is still awaiting its first snapshot.
    let mut snapshot = cx
        .update(|_, cx| view.update(cx, |view, _| view.live.snapshot.take()))
        .unwrap();
    // Reserve the new footer while retaining this test's original list viewport.
    cx.simulate_resize(size(px(800.), px(640.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    // The fixture's grouped worktrees stay contiguous, so w30 is the 31st row.
    const ROW: usize = 30;
    {
        let snapshot = Arc::make_mut(&mut snapshot);
        snapshot.focused_workspace_id = Some("w30".into());
        for workspace in &mut snapshot.workspaces {
            workspace.focused = workspace.workspace_id == "w30";
        }
    }
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.live.snapshot = Some(snapshot);
            cx.notify();
        })
    });
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    cx.update(|_, cx| {
        let view = view.read(cx);
        let spaces = &view.sidebar_scroll[0];
        let offset = spaces.offset().y;
        let row = spaces.bounds_for_item(ROW).unwrap();
        assert!(offset < px(0.), "focused workspace must scroll into view");
        assert!(row.top() + offset >= spaces.bounds().top(), "{row:?}");
        assert!(row.bottom() + offset <= spaces.bounds().bottom(), "{row:?}");
        // The fixture focuses no agent, so that list must stay where it was.
        assert_eq!(view.sidebar_scroll[1].offset().y, px(0.));
    });
    // While the selection holds, later frames must not fight manual scrolling.
    cx.update(|window, cx| {
        view.read(cx).sidebar_scroll[0].set_offset(point(px(0.), px(0.)));
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).sidebar_scroll[0].offset().y, px(0.));
    });
    // A new selection is revealed in turn, from wherever the list now sits.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.focused_workspace_id = Some("w20".into());
            for workspace in &mut snapshot.workspaces {
                workspace.focused = workspace.workspace_id == "w20";
            }
            cx.notify();
        })
    });
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    cx.update(|_, cx| {
        let view = view.read(cx);
        let spaces = &view.sidebar_scroll[0];
        let offset = spaces.offset().y;
        let row = spaces.bounds_for_item(20).unwrap();
        assert!(offset < px(0.), "a new selection must scroll into view");
        assert!(row.top() + offset >= spaces.bounds().top(), "{row:?}");
        assert!(row.bottom() + offset <= spaces.bounds().bottom(), "{row:?}");
    });

    // Selecting a visible neighbor must not move the list. A selection above or
    // below the viewport should land at the nearest edge, not always the bottom.
    for (id, row, edge) in [
        ("w19", 19, None),
        ("w0", 0, Some(false)),
        ("w4", 4, None),
        ("w5", 5, None),
        ("w30", 30, Some(true)),
        ("w4", 4, Some(false)),
        ("w5", 5, None),
    ] {
        let before = cx.update(|_, cx| view.read(cx).sidebar_scroll[0].offset());
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                snapshot.focused_workspace_id = Some(id.into());
                for workspace in &mut snapshot.workspaces {
                    workspace.focused = workspace.workspace_id == id;
                }
                cx.notify();
            });
        });
        cx.update(|window, cx| {
            window.refresh();
            full_draw(window, cx).clear(cx);
        });
        cx.update(|_, cx| {
            let spaces = &view.read(cx).sidebar_scroll[0];
            let bounds = spaces.bounds_for_item(row).unwrap();
            match edge {
                None => assert_eq!(spaces.offset(), before, "visible {id} must not scroll"),
                Some(false) => assert_eq!(bounds.top() + spaces.offset().y, spaces.bounds().top()),
                Some(true) => assert_eq!(
                    bounds.bottom() + spaces.offset().y,
                    spaces.bounds().bottom()
                ),
            }
        });
    }
}

#[gpui_kit::test]
fn healthy_connection_status_is_quiet_but_diagnostics_remain(cx: &mut gpui_kit::TestAppContext) {
    let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("connection-message").is_none());
    assert!(cx.debug_bounds("status-theme").is_some());
    for status in [
        crate::state::ConnectionStatus::Connected,
        crate::state::ConnectionStatus::Disconnected,
    ] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.live.status = status;
                view.local_error = Some("Local operation failed".into());
                view.live.error = Some("Connection interrupted".into());
                cx.notify();
            });
            full_draw(window, cx).clear(cx);
        });
        assert!(cx.debug_bounds("connection-message").unwrap().size.width > px(0.));
    }
}

#[gpui_kit::test]
fn the_agents_header_toggles_between_grouped_and_priority(cx: &mut gpui_kit::TestAppContext) {
    use crate::preferences::AgentSort;
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    // The second agent wants attention; only priority floats it to the top.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.agents[0].state_change_seq = 9;
            snapshot.agents[1].agent_status = AgentStatus::Blocked;
            snapshot.agents[1].state_change_seq = 1;
        })
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let (first, second) = ("row-agent-p0", "row-agent-p1");
    for (expected, top) in [(AgentSort::Grouped, first), (AgentSort::Priority, second)] {
        cx.update(|_, cx| assert_eq!(view.read(cx).agent_sort, expected));
        let (a, b) = (
            cx.debug_bounds(first).unwrap(),
            cx.debug_bounds(second).unwrap(),
        );
        let ordered = if top == first {
            a.top() < b.top()
        } else {
            b.top() < a.top()
        };
        assert!(ordered, "{expected:?}: {a:?} {b:?}");
        let sort = cx.debug_bounds("agents-sort").unwrap();
        cx.simulate_click(sort.center(), Default::default());
        cx.update(|window, cx| {
            window.refresh();
            full_draw(window, cx).clear(cx);
        });
    }
    // Toggling twice returns to the stored default without a daemon request.
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.agent_sort, AgentSort::Grouped);
        assert!(view.agent_sort_modified);
    });
}

/// Resting the pointer on a workspace opens the menu its right click opens,
/// once, and only after the pointer has both moved and settled. The behavior
/// is opt-in, so the test turns its feature flag on.
#[gpui_kit::test]
fn resting_on_a_workspace_opens_its_menu_once(cx: &mut gpui_kit::TestAppContext) {
    let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view.active = true;
        view.config.features.sidebar_hover_menu = true;
        view
    });
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let row = cx.debug_bounds("row-herdr").unwrap().center();
    let settle = |view: &Entity<HerdrWindow>,
                  cx: &mut gpui_kit::VisualTestContext,
                  elapsed: std::time::Duration| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.poll_hover_menu(std::time::Instant::now() + elapsed, window, cx);
            });
            full_draw(window, cx).clear(cx);
        });
    };

    // Entering the row alone is not a rest: the pointer has not moved yet.
    cx.simulate_mouse_move(row, None, Modifiers::default());
    assert!(
        view.read_with(cx, |view, _| view.hover.is_some()),
        "row armed"
    );
    settle(&view, cx, super::HOVER_MENU_DELAY);
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));

    // Drifting inside the row restarts the dwell rather than opening early.
    cx.simulate_mouse_move(row + point(px(8.), px(0.)), None, Modifiers::default());
    settle(&view, cx, std::time::Duration::ZERO);
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
    settle(&view, cx, super::HOVER_MENU_DELAY / 2);
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));

    settle(&view, cx, super::HOVER_MENU_DELAY);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(crate::menu::Page::Workspace));
        assert_eq!(crate::menu::workspace_tests::target_id(view), Some("w0"));
        // The same one-shot intent, spent: nothing is left armed behind it.
        assert!(view.hover.is_none());
        assert!(view.hover_menu.is_some());
    });
}

/// Scrolling slides another row under a still pointer without a hover event
/// of its own, so the row it entered can no longer speak for what it covers.
#[gpui_kit::test]
fn scrolling_the_spaces_list_abandons_a_rest(cx: &mut gpui_kit::TestAppContext) {
    let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view.active = true;
        view.config.features.sidebar_hover_menu = true;
        view
    });
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let row = cx.debug_bounds("row-herdr").unwrap().center();
    cx.simulate_mouse_move(row, None, Modifiers::default());
    cx.simulate_mouse_move(row + point(px(4.), px(4.)), None, Modifiers::default());
    let armed = view.read_with(cx, |view, _| {
        view.hover.as_ref().map(|hover| hover.workspace.clone())
    });
    assert!(armed.is_some(), "row armed");
    cx.update(|window, cx| {
        view.read(cx).sidebar_scroll[0].set_offset(point(px(0.), px(-40.)));
        view.update(cx, |view, cx| {
            view.poll_hover_menu(
                std::time::Instant::now() + super::HOVER_MENU_DELAY,
                window,
                cx,
            );
        });
    });
    view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        assert!(
            view.hover
                .as_ref()
                .is_none_or(|hover| Some(&hover.workspace) != armed.as_ref() && !hover.moved)
        );
    });
}

/// Without its feature flag, a resting pointer arms nothing and opens nothing:
/// only a right click still opens a space's menu.
#[gpui_kit::test]
fn resting_on_a_workspace_opens_nothing_by_default(cx: &mut gpui_kit::TestAppContext) {
    let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view.active = true;
        view
    });
    assert!(!crate::config::Config::default().features.sidebar_hover_menu);
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let row = cx.debug_bounds("row-herdr").unwrap().center();

    cx.simulate_mouse_move(row, None, Modifiers::default());
    cx.simulate_mouse_move(row + point(px(6.), px(0.)), None, Modifiers::default());
    assert!(
        view.read_with(cx, |view, _| view.hover.is_none()),
        "row armed"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // A stale rest from before the flag was turned off still expires.
            view.hover_workspace("w0", true, window);
            view.poll_hover_menu(
                std::time::Instant::now() + super::HOVER_MENU_DELAY * 2,
                window,
                cx,
            );
            assert!(view.menu.page.is_none());
            assert!(view.hover.is_none());
            assert!(view.hover_menu.is_none());
        });
        full_draw(window, cx).clear(cx);
    });
}

#[gpui_kit::test]
fn a_right_click_opens_the_workspace_menu_for_its_row(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::MouseButton;
    let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let row = cx.debug_bounds("row-agent-launcher").unwrap().center();
    cx.simulate_mouse_down(row, MouseButton::Right, Modifiers::default());
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(crate::menu::Page::Workspace));
        assert_eq!(crate::menu::workspace_tests::target_id(view), Some("w3"));
    });
}

#[gpui_kit::test]
fn a_left_click_navigates_to_the_row(cx: &mut gpui_kit::TestAppContext) {
    let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let row = cx
        .debug_bounds("row-herdr-gpui-sidebar-rendering-regression-investigation")
        .unwrap()
        .center();
    cx.simulate_click(row, Modifiers::default());
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.menu.page.is_none());
        assert!(view.focus.is_focused(window));
    });
}

#[gpui_kit::test]
fn the_fold_button_hides_and_shows_a_worktree_group(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    for collapsed in [true, false] {
        let parent = cx.debug_bounds("row-agent-launcher").unwrap();
        // The fold control is the last thing on the parent's row.
        let fold = point(parent.right() - px(20.), parent.center().y);
        cx.simulate_click(fold, Modifiers::default());
        cx.update(|window, cx| {
            window.refresh();
            full_draw(window, cx).clear(cx);
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.collapsed_repos.contains(REPO_KEY), collapsed);
            // Folding is the client's own view, never a navigation.
            assert!(view.pending_navigation.is_none());
        });
        assert_eq!(
            cx.debug_bounds("row-sidebar-child").is_none(),
            collapsed,
            "children follow the fold"
        );
    }
}

#[gpui_kit::test]
fn the_sidebar_width_follows_its_preference_and_survives_window_resizes(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (view, cx) = crate::test_support::add_window_view(cx, fixture_window);
    let draw = |cx: &mut gpui_kit::VisualTestContext| {
        for _ in 0..3 {
            cx.update(|window, cx| full_draw(window, cx).clear(cx));
            cx.run_until_parked();
        }
    };
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    assert_eq!(
        cx.debug_bounds("sidebar").map(|b| b.size.width),
        Some(px(232.))
    );
    // A stored width arrives after the first frames and is applied as-is.
    view.update(cx, |view, cx| {
        view.sidebar_width = Some(300.);
        cx.notify();
    });
    draw(cx);
    assert_eq!(
        cx.debug_bounds("sidebar").map(|b| b.size.width),
        Some(px(300.))
    );
    // The kit rescales panels with the window; the preference is in pixels.
    cx.simulate_resize(size(px(1200.), px(600.)));
    draw(cx);
    assert_eq!(
        cx.debug_bounds("sidebar").map(|b| b.size.width),
        Some(px(300.))
    );
}

#[gpui_kit::test]
fn dragging_the_sidebar_edge_stores_its_width(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::MouseButton;
    let (view, cx) = crate::test_support::add_window_view(cx, fixture_window);
    cx.simulate_resize(size(px(800.), px(600.)));
    for _ in 0..3 {
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        cx.run_until_parked();
    }
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    let edge = point(sidebar.right(), sidebar.center().y);
    cx.simulate_mouse_down(edge, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        edge + point(px(10.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        edge + point(px(60.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        edge + point(px(60.), px(0.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let width = view.sidebar_width.unwrap();
        assert!((width - 292.).abs() <= 1., "{width}");
        assert!(view.sidebar_modified);
    });
}

#[gpui_kit::test]
fn the_split_follows_its_preference_and_agents_can_hide(cx: &mut gpui_kit::TestAppContext) {
    let (view, cx) = crate::test_support::add_window_view(cx, fixture_window);
    let draw = |cx: &mut gpui_kit::VisualTestContext| {
        for _ in 0..3 {
            cx.update(|window, cx| full_draw(window, cx).clear(cx));
            cx.run_until_parked();
        }
    };
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update(cx, |view, cx| {
        view.sidebar_split = Some(0.25);
        cx.notify();
    });
    draw(cx);
    let spaces = cx.debug_bounds("spaces-scroll").unwrap();
    let agents = cx.debug_bounds("agents-section").unwrap();
    let share = spaces.size.height / (spaces.size.height + agents.size.height);
    assert!((share - 0.25).abs() < 0.05, "{share}");
    // Hiding agents gives the spaces list the whole height between the header
    // and the footer; the stored split is untouched.
    view.update(cx, |view, cx| {
        view.config.show_agents = false;
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("agents-section").is_none());
    assert!(cx.debug_bounds("spaces-scroll").unwrap().size.height > spaces.size.height * 2.);
    view.read_with(cx, |view, _| assert_eq!(view.sidebar_split, Some(0.25)));
}

#[gpui_kit::test]
fn the_settings_button_opens_preferences(cx: &mut gpui_kit::TestAppContext) {
    let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        crate::bind_keys(cx);
        fixture_window(window, cx)
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let settings = cx.debug_bounds("device-settings").unwrap().center();
    cx.simulate_click(settings, Modifiers::default());
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(crate::menu::Page::Preferences));
    });
}

#[gpui_kit::test]
fn holding_a_workspace_row_lifts_it_and_a_release_picks_the_gap(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::{MouseButton, point};

    let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.simulate_resize(size(px(800.), px(900.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let target = |view: &Entity<HerdrWindow>, cx: &mut gpui_kit::VisualTestContext| {
        view.read_with(cx, |view, _| {
            let drag = view.workspace_drag.as_ref()?;
            assert!(drag.lifted);
            Some(drag.target.as_ref()?.params())
        })
    };

    // A quick click stays a click.
    let first = cx.debug_bounds("row-herdr").unwrap();
    cx.simulate_mouse_down(first.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(first.center(), MouseButton::Left, Modifiers::default());
    cx.executor().advance_clock(super::reorder::LIFT_DELAY * 2);
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(view.workspace_drag.is_none());
        // Without a surface the click's navigation waits, which shows it ran.
        assert!(view.pending_navigation.take().is_some());
    });

    // Held in place, the row lifts without moving.
    cx.simulate_mouse_down(first.center(), MouseButton::Left, Modifiers::default());
    view.read_with(cx, |view, _| {
        assert!(!view.workspace_drag.as_ref().unwrap().lifted)
    });
    cx.executor().advance_clock(super::reorder::LIFT_DELAY);
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    view.read_with(cx, |view, _| {
        assert!(view.workspace_drag.as_ref().unwrap().lifted)
    });
    // Over its own place it would move nothing, so nothing shifts.
    assert_eq!(target(&view, cx), None);
    let second = cx
        .debug_bounds("row-herdr-gpui-sidebar-rendering-regression-investigation")
        .unwrap();
    assert_eq!(second.top(), first.bottom());

    // Past the second row's middle, it lands before the third.
    let below = point(first.center().x, second.bottom() - px(2.));
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(
        target(&view, cx),
        Some(serde_json::json!({"workspace_ids": ["w0"], "before_workspace_id": "w2"}))
    );
    // The lifted row follows the pointer.
    let lifted = cx.debug_bounds("row-herdr").unwrap();
    assert_eq!(lifted.top() - first.top(), below.y - first.center().y);
    assert_eq!(lifted.left(), first.left());
    // The second row closes the lifted one's place, opening the gap it
    // would land in, and the gaps are still measured where rows rest.
    let passed = "row-herdr-gpui-sidebar-rendering-regression-investigation";
    assert_eq!(cx.debug_bounds(passed).unwrap().top(), first.top());
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(
        target(&view, cx),
        Some(serde_json::json!({"workspace_ids": ["w0"], "before_workspace_id": "w2"}))
    );
    assert_eq!(cx.debug_bounds(passed).unwrap().top(), first.top());
    // The card's bottom edge passing the second row's resting middle is
    // enough, though that row now paints higher.
    let past = point(below.x, first.center().y + second.size.height / 2. + px(2.));
    cx.simulate_mouse_move(past, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(
        target(&view, cx),
        Some(serde_json::json!({"workspace_ids": ["w0"], "before_workspace_id": "w2"}))
    );
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));

    // The release is the drop, not a click on the row under it.
    cx.simulate_mouse_up(below, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    view.read_with(cx, |view, _| {
        assert!(view.workspace_drag.is_none());
        assert!(view.pending_navigation.is_none());
    });
    // Nothing was sent without a daemon, so every row is back in place.
    assert_eq!(cx.debug_bounds("row-herdr").unwrap(), first);
    assert_eq!(cx.debug_bounds(passed).unwrap(), second);

    // A linked worktree moves among its siblings only, and a drag lifts it
    // without waiting. The gap resolves against the lifted frame's layout.
    let child = cx.debug_bounds("row-sidebar-child").unwrap();
    cx.simulate_mouse_down(child.center(), MouseButton::Left, Modifiers::default());
    for rows in [1., 5.] {
        cx.simulate_mouse_move(
            point(child.center().x, child.bottom() + child.size.height * rows),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    }
    assert_eq!(
        target(&view, cx),
        Some(serde_json::json!({"workspace_ids": ["w4"], "before_workspace_id": "w6"}))
    );
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    view.read_with(cx, |view, _| assert!(view.workspace_drag.is_none()));
    assert_eq!(cx.debug_bounds("row-sidebar-child").unwrap(), child);
}
