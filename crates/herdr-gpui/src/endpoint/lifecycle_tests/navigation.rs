use super::*;

#[gpui::test]
fn workspace_menu_keeps_immediate_and_deferred_navigation(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for deferred in [false, true] {
        let (endpoint, mut server) = connected_endpoint("local");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.selected_endpoint = 0;
                view.endpoints = vec![endpoint];
                view.options = ConnectOptions::default();
                view.reset_selected();
                if deferred {
                    view.live.surface = None;
                }
                assert!(view.navigate_endpoint("local", NavigationTarget::Workspace("w1"), cx));
                view.open_workspace_menu("w1", Default::default(), window, cx);
                assert_eq!(view.menu.page, Some(crate::menu::Page::Workspace));
                if deferred {
                    assert!(view.pending_navigation.is_some());
                    view.live = view.endpoints[0].live.clone();
                    view.poll_endpoints(cx);
                }
                assert!(view.pending_navigation.is_none());
                assert!(!view.input_ready());
                assert_eq!(view.menu.page, Some(crate::menu::Page::Workspace));
                assert!(view.menu.focus.is_focused(window));
                // New input is still blocked while the menu owns focus.
                assert!(!view.navigate(NavigationTarget::Workspace("other"), cx));
            });
        });
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing workspace focus request");
        };
        let request: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], "workspace.focus");
        assert_eq!(request["params"], serde_json::json!({"workspace_id": "w1"}));
        server.respond(&request);
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing surface barrier");
        };
        let barrier: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(barrier["method"], Method::ClientShellSurfaceSet.as_str());
        view.update(cx, |view, cx| {
            let mut next = snapshot();
            next.revision += 1;
            next.focused_workspace_id = Some("w1".into());
            for workspace in &mut next.workspaces {
                workspace.focused = workspace.workspace_id == "w1";
            }
            {
                let mut state = view.endpoints[0].connection.inbox.lock().unwrap();
                state.apply(ClientEvent::Snapshot(Arc::new(next.clone())));
                state.apply(ClientEvent::Surface(surface(&next)));
                state.apply(ClientEvent::Response {
                    request_id: barrier["id"].as_str().unwrap().into(),
                    response: serde_json::json!({"result": {
                        "type": "client_shell_surface_set", "active": true,
                        "projection_revision": next.revision
                    }}),
                });
            }
            project_until(view, cx, "workspace selection", HerdrWindow::input_ready);
            assert_eq!(view.menu.page, Some(crate::menu::Page::Workspace));
            let selected = view.live.snapshot.as_ref().unwrap();
            assert_eq!(selected.focused_workspace_id.as_deref(), Some("w1"));
            assert!(
                selected
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.workspace_id == "w1" && workspace.focused)
            );
        });
    }
}

#[gpui::test]
fn saved_selection_waits_for_snapshot_without_overwriting_preference(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut remote, _server) = connected_endpoint("ssh:saved");
    let ready = remote.live.clone();
    remote.live.snapshot = None;
    remote.initial_surface = false;
    view.update(cx, |view, cx| {
        view.catalog.desired = Some("saved".into());
        view.catalog.initialized = true;
        view.catalog.restore_pending = true;
        // The catalog and then its connection can arrive long after startup.
        view.restore_selection(cx);
        assert!(view.catalog.restore_pending);
        view.endpoints.push(remote);
        for _ in 0..10 {
            view.restore_selection(cx);
            assert_eq!(view.selected_endpoint, 0);
            assert!(view.activation_deadline.is_none());
        }
        view.endpoints[1].live = ready;
        view.restore_selection(cx);
        assert_eq!(view.selected_endpoint, 1);
        assert!(!view.catalog.restore_pending);
        assert!(view.catalog.queued_write.is_none());
        // Automatic fallback is not a user choice and must not cause a loop.
        view.switch_endpoint(LOCAL, cx);
        view.restore_selection(cx);
        assert_eq!(view.selected_endpoint, 0);
        assert_eq!(view.catalog.desired.as_deref(), Some("saved"));
        assert!(view.catalog.queued_write.is_none());
        // An explicit Local click cancels even a not-yet-ready restore.
        view.catalog.restore_pending = true;
        view.select_endpoint(LOCAL, cx);
        view.restore_selection(cx);
        assert_eq!(view.catalog.desired, None);
        assert_eq!(view.selected_endpoint, 0);
    });
}

/// Another host changing redraws the window but leaves the selected host's
/// window state alone: a split request the window is still waiting on must
/// not vanish because a different endpoint had news, and the selected host's
/// own news still arrives.
#[gpui::test]
fn another_endpoint_changing_keeps_the_selected_window_state(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (selected, _server) = connected_endpoint("ssh:selected");
    let (other, _other_server) = connected_endpoint("ssh:other");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, selected);
            view.endpoints.push(other);
            view.poll_endpoints(cx);
            view.live.drag_request = Some("gpui-pending".into());

            view.endpoints[2].connection.inbox.lock().unwrap().dirty = true;
            view.poll_endpoints(cx);
            assert_eq!(view.live.drag_request.as_deref(), Some("gpui-pending"));

            view.endpoints[1].connection.inbox.lock().unwrap().error = Some("news".into());
            view.endpoints[1].connection.inbox.lock().unwrap().dirty = true;
            view.poll_endpoints(cx);
            assert_eq!(view.live.error.as_deref(), Some("news"));
        });
    });
}
