//! Herdr refuses every command but `client_shell.surface.set` from a
//! connection whose surface is inactive, so nothing else may reach one.
use super::*;

fn endpoint_request(server: &mut Server) -> serde_json::Value {
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("expected an endpoint request");
    };
    serde_json::from_str(&request).unwrap()
}

/// Answers an activation the way Herdr does. The worker keeps one request in
/// flight, so nothing queued behind the activation goes out until then.
fn acknowledge_activation(server: &mut Server, request: &serde_json::Value) {
    let id = request["id"].as_str().unwrap();
    let response = serde_json::json!({"id": id, "result": {
        "type": "client_shell_surface_set", "active": true,
        "projection_revision": snapshot().revision
    }});
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

/// Asserts that `handle`'s next frame is a focus report sent now, so nothing
/// was queued on that connection before it.
fn assert_nothing_queued(handle: &ClientHandle, server: &mut Server) {
    handle.set_focus(&snapshot().boot_id, false).unwrap();
    let message = server.receive();
    assert!(
        matches!(message, ClientMessage::ClientShellFocus { focused: false }),
        "unexpected frame on an inactive surface: {message:?}"
    );
}

#[gpui::test]
fn switching_devices_holds_commands_until_the_new_surface_is_activated(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (source, mut source_server) = connected_endpoint("ssh:source");
    let (mut target, mut target_server) = connected_endpoint("ssh:target");
    target.initial_surface = false;
    let source_inbox = source.connection.inbox.clone();
    let target_handle = target.connection.handle.clone().unwrap();
    view.update(cx, |view, cx| {
        view.endpoints[0].detached = true;
        view.endpoints.extend([source, target]);
        view.selected_endpoint = 1;
        view.options = ConnectOptions::default();
        view.reset_selected(cx);
        // Settings saved while the target is in the background.
        view.endpoints[2].reload_config().unwrap();
        assert!(view.select_endpoint("ssh:target", cx));
        // The source's release is not acknowledged yet, so the target's
        // activation waits, and so must the resize's tab claim.
        let now = Instant::now();
        for tick in 0..5 {
            view.poll_endpoints(cx);
            view.resize_at(now + Duration::from_secs(2) * tick);
        }
        assert!(!view.endpoints[2].initial_surface);
    });
    assert_nothing_queued(&target_handle, &mut target_server);

    assert!(matches!(
        source_server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    ));
    let release = endpoint_request(&mut source_server);
    assert_eq!(release["method"], Method::ClientShellSurfaceSet.as_str());
    assert_eq!(release["params"]["active"], false);
    source_inbox.lock().unwrap().apply(ClientEvent::Response {
        request_id: release["id"].as_str().unwrap().into(),
        response: serde_json::json!({"result": {
            "type": "client_shell_surface_set", "active": false, "projection_revision": 7
        }}),
    });
    view.update(cx, |view, cx| {
        project_until(view, cx, "target activation", |view| {
            view.endpoints[2].initial_surface
        });
        view.poll_endpoints(cx);
    });
    assert!(matches!(
        target_server.receive(),
        ClientMessage::ClientShellResize { .. }
    ));
    let activation = endpoint_request(&mut target_server);
    assert_eq!(activation["method"], Method::ClientShellSurfaceSet.as_str());
    assert_eq!(activation["params"]["active"], true);
    acknowledge_activation(&mut target_server, &activation);
    let reload = endpoint_request(&mut target_server);
    assert_eq!(reload["method"], Method::ServerReloadConfig.as_str());
    view.update(cx, |view, cx| view.poll_endpoints(cx));
    assert_nothing_queued(&target_handle, &mut target_server);
}

#[test]
fn a_full_queue_holds_the_reload_without_reporting_a_failure() {
    let (mut endpoint, _server) = connected_endpoint("ssh:full");
    // Nothing drains the queue while the connector waits at the gate.
    let (stream, _peer) = herdr_client::Stream::pair().unwrap();
    let (_open, gate) = std::sync::mpsc::channel::<()>();
    let client = herdr_client::connect_with_connector(
        endpoint.connection.target.clone(),
        ConnectOptions::default(),
        true,
        move |_, _| {
            let _ = gate.recv();
            Ok(stream)
        },
    )
    .unwrap();
    let boot = snapshot().boot_id;
    while client.handle.set_focus(&boot, true).is_ok() {}
    assert!(matches!(
        client.handle.set_focus(&boot, true),
        Err(herdr_client::Error::Full)
    ));
    endpoint.connection.handle = Some(client.handle);
    endpoint.reload_config().unwrap();
    assert!(endpoint.config_reload_pending);
    endpoint.flush_config_reload().unwrap();
    assert!(endpoint.config_reload_pending, "a later poll retries it");
}
