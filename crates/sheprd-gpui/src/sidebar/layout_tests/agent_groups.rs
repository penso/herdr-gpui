use super::*;
use crate::controls::Command;

/// A window listing `demo-coord`, `demo-12`, and `other-1`.
fn grouped(window: &mut Window, cx: &mut Context<SheprdWindow>) -> SheprdWindow {
    let mut view = fixture_window(window, cx);
    view.config.sidebar_style.coordinator_suffix = Some("-coord".into());
    let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    let agent = snapshot.agents[0].clone();
    snapshot.agents = ["demo-12", "other-1", "demo-coord"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| ClientShellAgent {
            pane_id: format!("p{index}"),
            name: Some(name.to_owned()),
            ..agent.clone()
        })
        .collect();
    view
}

fn rows(view: &SheprdWindow) -> Vec<String> {
    view.sidebar_agents()
        .into_iter()
        .map(|(_, pane)| pane.to_owned())
        .collect()
}

#[gpui::test]
fn the_header_filter_narrows_the_list_and_the_shortcuts_alike(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(grouped);
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("agents-group").is_some());
    view.read_with(cx, |view, _| {
        assert_eq!(rows(view), ["p0", "p1", "p2"]);
    });

    let click = |cx: &mut gpui::VisualTestContext| {
        let center = cx.debug_bounds("agents-group").unwrap().center();
        cx.simulate_click(center, Modifiers::default());
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    };
    click(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.agent_group.as_deref(), Some("demo"));
        // The coordinator leads, whatever the sort put first.
        assert_eq!(rows(view), ["p2", "p0"]);
        // Stepping reads the same rows, so it lands on the coordinator.
        let sidebar = view.sidebar_agents();
        assert_eq!(
            crate::sidebar::order::step(&sidebar, None, 0, Command::NextAgent),
            Some(0)
        );
    });
    // One group: the next click is back to all of them.
    click(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.agent_group, None);
        assert_eq!(rows(view).len(), 3);
    });
}

#[gpui::test]
fn without_a_suffix_nothing_changes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = grouped(window, cx);
        view.config.sidebar_style.coordinator_suffix = None;
        // Left over from before the setting was cleared.
        view.agent_group = Some("demo".into());
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("agents-group").is_none());
    view.read_with(cx, |view, _| assert_eq!(rows(view).len(), 3));
}

/// Other placements have no header to say a filter is on, so a filter must
/// not silently narrow what the shortcuts step through.
#[gpui::test]
fn a_filter_only_applies_where_the_panel_shows_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = grouped(window, cx);
        view.agent_group = Some("demo".into());
        view.config.agents_placement = crate::config::AgentsPlacement::Nested;
        view
    });
    view.read_with(cx, |view, _| {
        assert!(view.agent_groups().is_none());
        assert_eq!(rows(view).len(), 3);
    });
}

/// With the header turned off, the row still appears while a group hides
/// agents, and says no agent matches rather than that there are none.
#[gpui::test]
fn a_picked_group_keeps_the_hidden_header(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = grouped(window, cx);
        view.config.show_agents_header = false;
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    for (group, shown) in [(None, false), (Some("gone"), true)] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.agent_group = group.map(str::to_owned);
                cx.notify();
            });
            cx.default_global::<TextProbes>().0.clear();
            full_draw(window, cx).clear(cx);
        });
        assert_eq!(cx.debug_bounds("header-agents").is_some(), shown);
    }
    cx.update(|_, cx| {
        let probes = &cx.global::<TextProbes>().0;
        assert!(probes.contains_key("no matching agents"));
        assert!(probes.contains_key("gone"));
    });
}

#[gpui::test]
fn a_coordinator_row_trades_its_agent_mark_for_the_coordinators(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(grouped);
    view.read_with(cx, |view, _| {
        let snapshot = view.live.snapshot.as_deref().unwrap();
        let icon = |index: usize| {
            crate::sidebar::agents::agent_icon(
                &snapshot.agents[index],
                snapshot,
                &view.config.sidebar_style,
            )
        };
        assert_eq!(icon(2), crate::icons::AgentIcon::Coordinator);
        assert_eq!(
            icon(0),
            crate::icons::AgentIcon::from_identity(Some("claude"))
        );
    });
}

/// The group's name gives way before the sort does, however long it runs.
#[gpui::test]
fn a_long_group_name_leaves_the_sort_inside_a_narrow_header(cx: &mut gpui::TestAppContext) {
    let long = "a-coordinator-group-with-a-very-long-name-indeed";
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = grouped(window, cx);
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.agents[2].name = Some(format!("{long}-coord"));
        view.agent_group = Some(long.to_owned());
        view.sidebar_width = Some(160.);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    view.read_with(cx, |view, _| assert_eq!(rows(view), ["p2"]));
    let header = cx.debug_bounds("header-agents").unwrap();
    let group = cx.debug_bounds("agents-group").unwrap();
    let sort = cx.debug_bounds("agents-sort").unwrap();
    assert!(sort.right() <= header.right(), "{sort:?} past {header:?}");
    assert!(group.right() <= sort.left(), "{group:?} over {sort:?}");
}
