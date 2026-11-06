use super::*;
use sheprd_client::protocol::AgentStatus;

/// Go To rows of `palette` in list order, named by their destination.
fn go_to_rows(palette: &Palette) -> Vec<String> {
    palette
        .filtered
        .iter()
        .map(|hit| match &palette.entries[hit.index].action {
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
fn status_fixture(window: &mut Window, cx: &mut Context<SheprdWindow>) -> SheprdWindow {
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
    // alone as the heading of no agents, even when the query names it.
    cx.simulate_keystrokes("alt-b");
    cx.simulate_input("claude");
    assert_eq!(state(cx).3, ["w1", "w1:p1"]);
    view.update(cx, |view, cx| {
        let search = view.menu.palette.as_ref().unwrap().search.clone();
        search.update(cx, |input, cx| input.clear(cx));
    });
    cx.simulate_input("project w2");
    assert!(state(cx).3.is_empty());
    view.update(cx, |view, cx| {
        let search = view.menu.palette.as_ref().unwrap().search.clone();
        search.update(cx, |input, cx| input.clear(cx));
    });

    // Other filters ignore the status, and Go To keeps it.
    cx.simulate_keystrokes("ctrl-tab");
    let (filter, agent_filter, _, rows) = state(cx);
    assert_eq!(
        (filter, agent_filter),
        (Filter::Commands, AgentFilter::Blocked)
    );
    assert!(rows.iter().all(|row| row == "other") && !rows.is_empty());
    cx.simulate_keystrokes("ctrl-shift-tab");
    assert_eq!(state(cx).3, ["w1", "w1:p1"]);

    // Plain letters are search text, not status shortcuts.
    cx.simulate_input("b");
    let (_, agent_filter, query, _) = state(cx);
    assert_eq!((agent_filter, query.as_str()), (AgentFilter::Blocked, "b"));
}

#[gpui::test]
fn status_chips_show_only_in_go_to_and_choose_a_status(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(status_fixture);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_palette(Filter::Commands, window, cx)
        });
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("palette-agent-filter-working").is_none());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.set_palette_filter(Filter::Navigation, cx)
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
