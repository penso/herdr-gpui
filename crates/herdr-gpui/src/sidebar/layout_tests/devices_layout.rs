use super::*;
use crate::{config::LayoutMode, state::ConnectionStatus};

/// This machine with the fixture's two working agents, and a remote host
/// with one blocked agent, both connected, in the Devices layout.
fn devices_window(
    cx: &mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &mut gpui::VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.layout.mode = LayoutMode::Devices;
            view.live.status = ConnectionStatus::Connected;
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:test".into(),
                "Build box".into(),
                ConnectTarget::Ssh {
                    target: "unused".into(),
                    session: "default".into(),
                },
                true,
            );
            let mut listing = snapshot(1);
            listing.agents.truncate(1);
            listing.agents[0].pane_id = "remote-pane".into();
            listing.agents[0].agent_status = AgentStatus::Blocked;
            remote.live.snapshot = Some(Arc::new(listing));
            remote.live.status = ConnectionStatus::Connected;
            view.endpoints.push(remote);
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(700.)));
    // The tick gives the layout its search field.
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_devices_overview(cx)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    (view, cx)
}

#[gpui::test]
fn devices_list_their_agents_under_their_headers(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = devices_window(cx);
    assert!(cx.debug_bounds("header-devices").is_some());
    assert!(cx.debug_bounds("device-tree-search").is_some());
    // Agents sit in the tree, so neither workspaces nor the agents panel show.
    assert!(cx.debug_bounds("header-agents").is_none());
    assert!(cx.debug_bounds("row-herdr").is_none());

    let local = cx.debug_bounds("host-local").unwrap();
    let remote = cx.debug_bounds("host-ssh:test").unwrap();
    let local_agent = cx.debug_bounds("agent-local-p0").unwrap();
    let remote_agent = cx.debug_bounds("agent-ssh:test-remote-pane").unwrap();
    assert!(local.bottom() <= local_agent.top());
    assert!(local_agent.bottom() <= remote.top());
    assert!(remote.bottom() <= remote_agent.top());
    // Agents step in under their device's header.
    let name = cx.debug_bounds("host-working-local").unwrap();
    assert!(name.right() <= local.right());
    assert!(cx.debug_bounds("host-working-ssh:test").is_some());
}

#[gpui::test]
fn the_search_keeps_only_matching_devices(cx: &mut gpui::TestAppContext) {
    let (view, cx) = devices_window(cx);
    let input = view.read_with(cx, |view, _| {
        view.devices_overview
            .tree_search
            .as_ref()
            .unwrap()
            .input
            .clone()
    });
    cx.update(|window, cx| window.focus(&input.read(cx).focus.clone(), cx));
    cx.simulate_input("build");
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("host-local").is_none());
    assert!(cx.debug_bounds("host-ssh:test").is_some());
    assert!(cx.debug_bounds("agent-ssh:test-remote-pane").is_some());
}

#[gpui::test]
fn other_layouts_drop_the_search(cx: &mut gpui::TestAppContext) {
    let (view, cx) = devices_window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.layout.mode = LayoutMode::default();
            view.poll_devices_overview(cx);
        })
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(view.read_with(cx, |view, _| view.devices_overview.tree_search.is_none()));
    assert!(cx.debug_bounds("header-spaces").is_some());
    assert!(cx.debug_bounds("device-tree-search").is_none());
}

/// Configured `[sidebar_layout.agents]` rows reach the tree, as they reach
/// the agents panel in every other layout.
#[gpui::test]
fn configured_agent_rows_apply_under_each_device(cx: &mut gpui::TestAppContext) {
    let (view, cx) = devices_window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.sidebar_layout = toml::from_str(
                r#"
                [agents]
                rows = [["state_icon", "workspace"], ["agent"], ["state_text"]]
            "#,
            )
            .unwrap();
            cx.notify();
        })
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let header = cx.debug_bounds("host-local").unwrap();
    let row = cx.debug_bounds("agent-local-p0").unwrap();
    let third = cx.debug_bounds("line-agent-p0-2").unwrap();
    assert!(header.bottom() <= row.top());
    assert!(
        row.contains(&third.origin),
        "the third configured line is drawn"
    );
}

/// The Devices layout has one search field: its own, which narrows the tree
/// in place, stands in for the sidebar's, and the setting hides either.
#[gpui::test]
fn one_search_field_and_one_setting_for_both(cx: &mut gpui::TestAppContext) {
    let (view, cx) = devices_window(cx);
    assert!(cx.debug_bounds("device-tree-search").is_some());
    assert!(cx.debug_bounds("sidebar-search").is_none());

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.show_sidebar_search = false;
            cx.notify();
        })
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("device-tree-search").is_none());
    assert!(cx.debug_bounds("sidebar-search").is_none());
    // A hidden field's text no longer filters the tree.
    assert!(cx.debug_bounds("host-local").is_some());

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.show_sidebar_search = true;
            view.config.layout.mode = LayoutMode::default();
            cx.notify();
        })
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("sidebar-search").is_some());
    assert!(cx.debug_bounds("device-tree-search").is_none());
}
