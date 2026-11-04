use super::*;

#[gpui::test]
fn hiding_the_agents_header_keeps_the_list_and_drops_the_sort(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(800.), px(600.)));
    for show_header in [true, false, true] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.config.show_agents_header = show_header;
                cx.notify();
            });
            cx.default_global::<TextProbes>().0.clear();
            window.refresh();
            full_draw(window, cx).clear(cx);
        });
        assert_eq!(cx.debug_bounds("header-agents").is_some(), show_header);
        assert_eq!(cx.debug_bounds("agents-sort").is_some(), show_header);
        assert!(cx.debug_bounds("agents-section").is_some());
        cx.update(|_, cx| {
            assert!(cx.global::<TextProbes>().0.contains_key("Claude Code"));
        });
    }
}

/// A plugin's agent view is named in that row, the only sign the list is
/// filtered, so the row stays while the view is active.
#[gpui::test]
fn a_plugin_agent_view_keeps_the_hidden_header(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(800.), px(600.)));
    for label in [None, Some("review".to_owned()), None] {
        let shown = label.is_some();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.config.show_agents_header = false;
                Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agent_view_label = label;
                cx.notify();
            });
            window.refresh();
            full_draw(window, cx).clear(cx);
        });
        assert_eq!(cx.debug_bounds("header-agents").is_some(), shown);
        assert_eq!(cx.debug_bounds("agents-sort").is_some(), shown);
    }
}

/// Agent rows come from every listed host, so a plugin view on a host other
/// than the selected one also keeps the hidden header.
#[gpui::test]
fn a_plugin_agent_view_on_another_host_keeps_the_hidden_header(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.show_agents_header = false;
        let mut remote = crate::endpoint::Endpoint::new(
            "ssh:test".into(),
            "Remote".into(),
            ConnectTarget::Socket("/unused-agents-header-test.sock".into()),
            true,
        );
        remote.live.snapshot = view.live.snapshot.clone();
        view.endpoints.push(remote);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    for label in [None, Some("review".to_owned()), None] {
        let shown = label.is_some();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let remote = view.endpoints[1].live.snapshot.as_mut().unwrap();
                Arc::make_mut(remote).agent_view_label = label;
                cx.notify();
            });
            window.refresh();
            full_draw(window, cx).clear(cx);
        });
        assert_eq!(view.read_with(cx, |view, _| view.selected_endpoint), 0);
        assert_eq!(cx.debug_bounds("header-agents").is_some(), shown);
    }
}
