use super::*;
use crate::{config::AgentsPlacement, state::ConnectionStatus};

/// Two hosts, each with two workspaces and an agent in each of them, so rows
/// carry their `workspace-…` and `agent-…` selectors.
fn two_hosts(window: &mut Window, cx: &mut Context<SheprdWindow>) -> SheprdWindow {
    let mut view = fixture_window(window, cx);
    let listing = snapshot(2);
    view.live.snapshot = Some(Arc::new(listing.clone()));
    view.live.status = ConnectionStatus::Connected;
    let mut remote = crate::endpoint::Endpoint::new(
        "ssh:test".into(),
        "Remote".into(),
        ConnectTarget::Ssh {
            target: "unused".into(),
            session: "default".into(),
        },
        true,
    );
    remote.live.snapshot = Some(Arc::new(listing));
    remote.live.status = ConnectionStatus::Connected;
    view.endpoints.push(remote);
    view
}

/// Per host: its two workspace rows, then the agent row of each.
const HOSTS: [[&str; 4]; 2] = [
    [
        "workspace-local-w0",
        "workspace-local-w1",
        "agent-local-p0",
        "agent-local-p1",
    ],
    [
        "workspace-ssh:test-w0",
        "workspace-ssh:test-w1",
        "agent-ssh:test-p0",
        "agent-ssh:test-p1",
    ],
];

fn show(
    view: &Entity<SheprdWindow>,
    (show_agents, placement): (bool, AgentsPlacement),
    cx: &mut gpui::VisualTestContext,
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.show_agents = show_agents;
            view.config.agents_placement = placement;
            cx.notify();
        });
        full_draw(window, cx).clear(cx);
    });
}

#[gpui::test]
fn nested_agents_sit_under_their_workspace_and_drop_the_agents_section(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| two_hosts(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(800.)));

    show(&view, (true, AgentsPlacement::Nested), cx);
    assert!(cx.debug_bounds("agents-section").is_none());
    for [first, second, under_first, under_second] in HOSTS {
        let host = first;
        let [first, second, under_first, under_second] =
            [first, second, under_first, under_second].map(|row| cx.debug_bounds(row).unwrap());
        assert!(first.bottom() <= under_first.top(), "{host}");
        assert!(under_first.bottom() <= second.top(), "{host}");
        assert!(second.bottom() <= under_second.top(), "{host}");
        // The workspace row names the place, so the agent takes one line.
        assert!(under_first.size.height < first.size.height, "{host}");
        // A step in from the workspace, and inside the sidebar.
        assert!(under_first.left() >= first.left(), "{host}");
        assert!(under_first.right() <= cx.debug_bounds("sidebar").unwrap().right());
    }
    // The spaces list takes the height the agents section had.
    assert_eq!(
        cx.debug_bounds("spaces-section").unwrap().size.height,
        cx.debug_bounds("sidebar").unwrap().size.height
            - cx.debug_bounds("device-footer").unwrap().size.height
    );

    show(&view, (true, AgentsPlacement::Panel), cx);
    let agents = cx.debug_bounds("agents-section").unwrap();
    for [_, _, agent, _] in HOSTS {
        assert!(
            agents.contains(&cx.debug_bounds(agent).unwrap().center()),
            "{agent}"
        );
    }

    // Hidden agents stay hidden wherever they would be placed.
    show(&view, (false, AgentsPlacement::Nested), cx);
    assert!(cx.debug_bounds("agents-section").is_none());
    assert!(cx.debug_bounds("agent-local-p0").is_none());
    assert!(cx.debug_bounds("workspace-local-w0").is_some());
}

/// `two_hosts`, with this host's agents replaced by `count` in its first
/// workspace, none focused.
fn crowded(
    count: usize,
) -> impl FnOnce(&mut Window, &mut Context<SidebarFixture>) -> SidebarFixture {
    move |window, cx| {
        let view = cx.new(|cx| {
            let mut view = two_hosts(window, cx);
            view.config.agents_placement = AgentsPlacement::Nested;
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            let template = snapshot.agents[0].clone();
            snapshot.agents = (0..count)
                .map(|i| {
                    let mut agent = template.clone();
                    agent.pane_id = format!("p{i}");
                    agent
                })
                .collect();
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    }
}

fn settle(cx: &mut gpui::VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        cx.run_until_parked();
    }
}

#[gpui::test]
fn the_focused_nested_agent_scrolls_into_view_once(cx: &mut gpui::TestAppContext) {
    use gpui::{ScrollDelta, ScrollWheelEvent};

    let (fixture, cx) = cx.add_window_view(crowded(30));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(400.)));
    settle(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.agents[29].focused = true;
            cx.notify();
        })
    });
    settle(cx);
    let list = cx.debug_bounds("spaces-scroll").unwrap();
    let focused = cx.debug_bounds("agent-local-p29").unwrap();
    assert!(
        list.top() <= focused.top() && focused.bottom() <= list.bottom(),
        "{focused:?} in {list:?}"
    );
    // Scrolling away from an unchanged focus is the user's to keep.
    cx.simulate_event(ScrollWheelEvent {
        position: list.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(200.))),
        ..Default::default()
    });
    settle(cx);
    let moved = cx.debug_bounds("agent-local-p29").unwrap();
    assert!(moved.top() > focused.top(), "{moved:?} after {focused:?}");
}

#[gpui::test]
fn nested_agents_keep_the_configured_row_gap(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(crowded(2));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(800.)));
    let mut spacing = Vec::new();
    for gap in [0, 2] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.config.usage.inline = true;
                view.config.sidebar_layout =
                    toml::from_str(&format!("[agents]\nrows = [[\"agent\"]]\nrow_gap = {gap}"))
                        .unwrap();
                cx.notify();
            })
        });
        settle(cx);
        let first = cx.debug_bounds("agent-local-p0").unwrap();
        let second = cx.debug_bounds("agent-local-p1").unwrap();
        spacing.push(second.top() - first.bottom());
    }
    assert!(spacing[1] > spacing[0], "{spacing:?}");
}

/// Search results replace the spaces list, so a nested row never paints
/// while they show: the reveal waits for the list to return instead of
/// asking for frames, then brings the agent into view.
#[gpui::test]
fn search_results_hold_the_nested_reveal_until_the_list_returns(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(crowded(30));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(400.)));
    settle(cx);
    cx.update(|window, cx| {
        let focus = view.read(cx).sidebar_search.input.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    cx.simulate_input("herdr");
    settle(cx);
    assert!(view.read_with(cx, |view, _| view.sidebar_search.query().is_some()));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.agents[29].focused = true;
            cx.notify();
        })
    });
    settle(cx);
    assert!(cx.debug_bounds("agent-local-p29").is_none());
    assert!(!view.read_with(cx, |view, _| view.sidebar_nested_reveal.retrying()));

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.clear_heading_search(super::super::search::Heading::Spaces, window, cx)
        });
    });
    settle(cx);
    let list = cx.debug_bounds("spaces-scroll").unwrap();
    let focused = cx.debug_bounds("agent-local-p29").unwrap();
    assert!(
        list.top() <= focused.top() && focused.bottom() <= list.bottom(),
        "{focused:?} in {list:?}"
    );
}

/// A middle worktree's trunk runs on past the agents nested under it to the
/// next sibling, a selected one included: it is drawn after them, so their
/// fill cannot cover it.
#[gpui::test]
fn a_childs_trunk_runs_past_its_nested_agents(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.agents_placement = AgentsPlacement::Nested;
        let mut listing = snapshot(6);
        // w4 is the group's middle child; its agent is the focused one.
        listing.agents[0].workspace_id = "w4".into();
        listing.agents[0].focused = true;
        view.live.snapshot = Some(Arc::new(listing));
        view.live.status = ConnectionStatus::Connected;
        view.live.supports_surface = true;
        view.endpoints[0].live = view.live.clone();
        view
    });
    cx.simulate_resize(size(px(800.), px(900.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let agent = cx.debug_bounds("row-agent-p0").unwrap();
    let trunk = cx.debug_bounds("trunk-local-w4").unwrap();
    let next = cx
        .debug_bounds("row-sidebar-child-with-a-long-readable-branch-name")
        .unwrap();
    assert!(trunk.top() <= agent.top() && agent.bottom() <= trunk.bottom());
    assert_eq!(trunk.bottom(), next.top());
}
