use super::*;

fn reply(server: &mut Server, request: &serde_json::Value, body: serde_json::Value) {
    let id = request["id"].as_str().unwrap();
    let mut response = serde_json::json!({"id": id});
    response
        .as_object_mut()
        .unwrap()
        .extend(body.as_object().unwrap().clone());
    write_message(
        &mut server.stream,
        &ServerMessage::ClientShellEndpointResponseChunk {
            boot_id: snapshot().boot_id,
            request_id: id.into(),
            final_chunk: true,
            data: serde_json::to_vec(&response).unwrap(),
        },
        MAX_GRAPHICS_FRAME_SIZE,
    )
    .unwrap();
}

fn request(server: &mut Server) -> serde_json::Value {
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing API request");
    };
    serde_json::from_str(&request).unwrap()
}

/// Navigates to `destination` with both requests on the wire before either
/// response, then answers the activation first, as Herdr's response bridge does.
fn navigate_with_early_activation(
    cx: &mut gpui::TestAppContext,
    focus_ok: bool,
) -> (
    gpui::Entity<SheprdWindow>,
    &mut gpui::VisualTestContext,
    Server,
) {
    // A stable release whose focus and activation ordering was checked.
    let (endpoint, mut server) = connected_endpoint_reporting("focus-pipeline", Some("0.9.5"));
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.read_with(cx, |fixture, _| fixture.0.clone());
    view.update(cx, |view, cx| {
        view.endpoints = vec![endpoint];
        view.options = ConnectOptions::default();
        view.reset_selected(cx);
        assert!(view.input_ready());
        assert!(view.navigate(NavigationTarget::Pane("destination"), cx));
        assert!(!view.input_ready());
    });
    let focus = request(&mut server);
    assert_eq!(focus["method"], "pane.focus");
    let activation = request(&mut server);
    assert_eq!(activation["method"], Method::ClientShellSurfaceSet.as_str());
    let mut next = snapshot();
    next.revision += 1;
    if focus_ok {
        next.focused_pane_id = Some("destination".into());
    }
    reply(
        &mut server,
        &activation,
        serde_json::json!({"result": {
            "type": "client_shell_surface_set", "active": true,
            "projection_revision": next.revision,
        }}),
    );
    view.update(cx, |view, cx| {
        {
            let mut inbox = view.endpoints[0].connection.inbox.lock().unwrap();
            inbox.apply(ClientEvent::Snapshot(Arc::new(next.clone())));
            inbox.apply(ClientEvent::Surface(surface(&next)));
        }
        project_until(view, cx, "projection at the activation floor", |view| {
            view.live.snapshot.as_ref().unwrap().revision == next.revision
        });
        // The worker holds the activation until focus is answered.
        assert!(view.live.activation.as_ref().unwrap().revision.is_none());
        assert!(!view.input_ready());
    });
    let body = if focus_ok {
        serde_json::json!({"result": {"type": "pane_info", "pane": {"pane_id": "destination"}}})
    } else {
        serde_json::json!({"error": {"code": "pane_not_found", "message": "pane not found"}})
    };
    reply(&mut server, &focus, body);
    view.update(cx, |view, cx| {
        project_until(view, cx, "activation acknowledged", |view| {
            view.live
                .activation
                .as_ref()
                .is_some_and(|activation| activation.revision == Some(next.revision))
        });
    });
    (view, cx, server)
}

#[gpui::test]
fn pipelined_activation_resumes_input_only_after_focus_ack_and_coherent_target(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx, mut server) = navigate_with_early_activation(cx, true);
    view.update(cx, |view, cx| {
        project_until(view, cx, "navigation completed", SheprdWindow::input_ready);
        view.send(
            ClientPaneInputEvent::TextCommit("destination only".into()),
            cx,
        );
    });
    let ClientMessage::ClientShellPaneInput { pane_id, .. } = server.receive() else {
        panic!("missing input after coherent focus");
    };
    assert_eq!(pane_id, "destination");
}

#[gpui::test]
fn failed_focus_keeps_input_fenced_despite_an_early_activation(cx: &mut gpui::TestAppContext) {
    let (view, cx, mut server) = navigate_with_early_activation(cx, false);
    view.update(cx, |view, cx| {
        // The floor is acknowledged, but the requested target never arrived.
        assert!(!view.input_ready());
        view.send(
            ClientPaneInputEvent::TextCommit("not the old pane".into()),
            cx,
        );
        view.endpoints[0]
            .connection
            .handle
            .as_ref()
            .unwrap()
            .set_focus(&snapshot().boot_id, false)
            .unwrap();
    });
    // The FIFO sentinel arrives with no pane input ahead of it.
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
}

#[gpui::test]
fn an_unverified_daemon_keeps_activation_behind_the_focus_response(cx: &mut gpui::TestAppContext) {
    let (endpoint, mut server) = connected_endpoint_reporting("focus-serial", Some("0.9.6"));
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.read_with(cx, |fixture, _| fixture.0.clone());
    view.update(cx, |view, cx| {
        view.endpoints = vec![endpoint];
        view.options = ConnectOptions::default();
        view.reset_selected(cx);
        assert!(view.navigate(NavigationTarget::Pane("destination"), cx));
        view.endpoints[0]
            .connection
            .handle
            .as_ref()
            .unwrap()
            .set_focus(&snapshot().boot_id, false)
            .unwrap();
    });
    let focus = request(&mut server);
    assert_eq!(focus["method"], "pane.focus");
    // The marker queued behind the activation proves nothing was written yet.
    server
        .stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    loop {
        match read_message(&mut server.stream, MAX_FRAME_SIZE) {
            Ok(ClientMessage::ClientShellHostTheme { .. }) => {}
            Ok(other) => panic!("written before the focus response: {other:?}"),
            Err(_) => break,
        }
    }
    server
        .stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    server.respond(&focus);
    assert_eq!(
        request(&mut server)["method"],
        Method::ClientShellSurfaceSet.as_str()
    );
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
}
