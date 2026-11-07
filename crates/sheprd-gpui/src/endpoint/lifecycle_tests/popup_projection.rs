use super::*;

fn popup_frame(frame: &PaneSurfaceFrame) -> PaneSurfaceFrame {
    let mut frame = frame.clone();
    frame.surface_revision += 1;
    frame.popup = Some(Box::new(ClientShellPopupSurface {
        terminal_id: "public-popup".into(),
        title: String::new(),
        width: None,
        height: None,
        frame: frame.frame.clone(),
        mouse_reporting: false,
        sgr_pixel_mouse: false,
        pixel_width: 800,
        pixel_height: 480,
    }));
    frame
}

#[gpui::test]
fn external_popup_at_unchanged_revision_never_receives_earlier_pane_input(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("external-popup");
    view.update(cx, |view, cx| {
        prepare_mouse(view, endpoint, cx);
        view.send(ClientPaneInputEvent::TextCommit("before".into()), cx);
        let frame = popup_frame(view.live.surface.as_ref().unwrap());
        view.endpoints[1]
            .connection
            .inbox
            .lock()
            .unwrap()
            .apply(ClientEvent::Surface(Arc::new(frame)));
        project_until(view, cx, "popup at unchanged revision", |view| {
            view.live.surface.as_ref().unwrap().popup.is_some()
        });
        view.send(ClientPaneInputEvent::TextCommit("after".into()), cx);
    });
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![ClientPaneInputEvent::TextCommit("before".into())],
        }
    );
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPopupInput {
            terminal_id: "public-popup".into(),
            events: vec![ClientPaneInputEvent::TextCommit("after".into())],
        }
    );
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

#[gpui::test]
fn local_popup_command_keeps_its_barrier_across_metadata_staging(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("local-popup");
    view.update(cx, |view, cx| {
        prepare_mouse(view, endpoint, cx);
        let mut next = (**view.live.snapshot.as_ref().unwrap()).clone();
        next.commands[0].action = ClientShellCommandAction::Popup;
        view.live.snapshot = Some(Arc::new(next.clone()));
        let inbox = view.endpoints[1].connection.inbox.clone();
        inbox.lock().unwrap().snapshot = view.live.snapshot.clone();
        next.revision += 1;
        inbox
            .lock()
            .unwrap()
            .apply(ClientEvent::Snapshot(Arc::new(next)));
        project_until(view, cx, "staged metadata", |view| {
            view.live.pending_snapshot.is_some()
        });
        view.invoke_custom_command("command-v1", ClientShellCommandAction::Popup, cx);
        assert!(!view.input_ready());
        view.send(ClientPaneInputEvent::TextCommit("before ack".into()), cx);
        assert_eq!(view.pending_input.len(), 0);
        view.endpoints[1]
            .connection
            .handle
            .as_ref()
            .unwrap()
            .set_focus(&view.live.snapshot.as_ref().unwrap().boot_id, false)
            .unwrap();
    });
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing popup invocation");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], Method::CommandInvoke.as_str());
    server.respond(&request);
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing surface barrier");
    };
    let barrier: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(barrier["method"], Method::ClientShellSurfaceSet.as_str());
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
    view.update(cx, |view, cx| {
        let mut next = (**view.live.snapshot.as_ref().unwrap()).clone();
        next.revision += 2;
        let frame = popup_frame(&surface(&next));
        let inbox = view.endpoints[1].connection.inbox.clone();
        {
            let mut inbox = inbox.lock().unwrap();
            inbox.apply(ClientEvent::Snapshot(Arc::new(next.clone())));
            inbox.apply(ClientEvent::Surface(Arc::new(frame)));
        }
        project_until(view, cx, "popup before acknowledgement", |view| {
            view.live
                .surface
                .as_ref()
                .is_some_and(|frame| frame.popup.is_some())
        });
        assert!(!view.input_ready());
        view.send(ClientPaneInputEvent::TextCommit("before floor".into()), cx);
        inbox.lock().unwrap().apply(ClientEvent::Response {
            request_id: barrier["id"].as_str().unwrap().into(),
            response: serde_json::json!({"result": {
                "type": "client_shell_surface_set", "active": true,
                "projection_revision": next.revision
            }}),
        });
        project_until(view, cx, "popup barrier", SheprdWindow::input_ready);
        view.send(ClientPaneInputEvent::TextCommit("after".into()), cx);
    });
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPopupInput {
            terminal_id: "public-popup".into(),
            events: vec![ClientPaneInputEvent::TextCommit("after".into())],
        }
    );
}
