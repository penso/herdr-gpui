use super::*;
use crate::sidebar::layout_tests::{fixture_window, full_draw};

/// The next message past any resize and the `tab.focus` that claims the tab
/// back after each, which is answered: one request is in flight at a time,
/// so an unanswered one would hold back every request after it. This test
/// renders the real canvas, so the window measures and settles its surface
/// on its own: when, and to what size, depends on the platform's font and on
/// scheduling, not on the hover focus tested here.
fn past_resizes(server: &mut Server) -> ClientMessage {
    loop {
        let message = server.receive();
        match &message {
            ClientMessage::ClientShellResize { .. } => continue,
            ClientMessage::ClientShellEndpointRequest { request, .. } => {
                let request: serde_json::Value = serde_json::from_str(request).unwrap();
                if request["method"] == Method::TabFocus.as_str() {
                    server.respond(&request);
                    continue;
                }
            }
            _ => {}
        }
        return message;
    }
}

#[gpui::test]
fn rendered_hover_resumes_typing_in_the_hovered_pane(cx: &mut gpui::TestAppContext) {
    let (endpoint, mut server) = connected_endpoint("ssh:hover-render");
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    // Let the actual canvas measure its geometry before the connected
    // endpoint is selected. Measured while connected, a first draw slower
    // than the resize settle time (fonts loading on a cold run) sends that
    // resize to the server ahead of the input this test reads.
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let (mut next, mut frame) = view.update(cx, |view, cx| {
        let measured = view.options;
        prepare_mouse(view, endpoint, cx);
        view.options = measured;
        let mut frame = (**view.live.surface.as_ref().unwrap()).clone();
        let size = view.options.surface_size;
        assert!(size.cols > 4 && size.rows > 2);
        frame.frame.width = size.cols;
        frame.frame.height = size.rows;
        frame.surface_revision += 1;
        let split = size.cols / 2;
        frame.panes[0].rect = SurfaceRect {
            x: 0,
            y: 0,
            width: split,
            height: size.rows,
        };
        frame.panes[1].rect = SurfaceRect {
            x: split,
            y: 0,
            width: size.cols - split,
            height: size.rows,
        };
        for pane in &mut frame.panes {
            pane.inner_rect = pane.rect;
            pane.mouse_reporting = false;
        }
        view.live.surface = Some(Arc::new(frame.clone()));
        view.last_queued_options = Some(view.options);
        view.pending_resize = None;
        let next = (**view.live.snapshot.as_ref().unwrap()).clone();
        {
            let mut state = view.endpoints[1].connection.inbox.lock().unwrap();
            state.apply(ClientEvent::Snapshot(Arc::new(next.clone())));
            state.apply(ClientEvent::Surface(Arc::new(frame.clone())));
        }
        assert!(view.config.focus_follows_mouse);
        assert!(view.input_ready());
        cx.notify();
        (next, frame)
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| window.focus(&view.focus, cx));
        full_draw(window, cx).clear(cx);
    });
    let positions = view.read_with(cx, |view, _| {
        frame
            .panes
            .iter()
            .map(|pane| {
                view.bounds.origin
                    + point(
                        px(
                            (f32::from(pane.inner_rect.x) + f32::from(pane.inner_rect.width) / 2.)
                                * view.cell_width,
                        ),
                        px(f32::from(pane.inner_rect.height) / 2.
                            * view.config.terminal.line_height()),
                    )
            })
            .collect::<Vec<_>>()
    });
    cx.simulate_mouse_move(positions[0], None, gpui::Modifiers::default());
    cx.simulate_input("b");
    assert_eq!(
        past_resizes(&mut server),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![ClientPaneInputEvent::TextCommit("b".into())],
        }
    );

    // Use the hit-tested render listener, not focus_hovered_pane directly.
    cx.simulate_mouse_move(positions[1], None, gpui::Modifiers::default());
    view.read_with(cx, |view, _| assert!(!view.input_ready()));
    cx.simulate_input("d");
    let ClientMessage::ClientShellEndpointRequest { request, .. } = past_resizes(&mut server)
    else {
        panic!("missing hover focus request");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], Method::PaneFocus.as_str());
    assert_eq!(request["params"], serde_json::json!({"pane_id": "w1:p2"}));
    server.respond(&request);
    let ClientMessage::ClientShellEndpointRequest { request, .. } = past_resizes(&mut server)
    else {
        panic!("missing ordered surface barrier");
    };
    let barrier: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(barrier["method"], Method::ClientShellSurfaceSet.as_str());
    next.revision += 1;
    next.focused_pane_id = Some("w1:p2".into());
    for pane in &mut next.panes {
        pane.focused = pane.pane_id == "w1:p2";
    }
    frame.projection_revision = next.revision;
    frame.surface_revision += 1;
    for pane in &mut frame.panes {
        pane.focused = pane.pane_id == "w1:p2";
    }
    view.update(cx, |view, cx| {
        {
            let mut state = view.endpoints[1].connection.inbox.lock().unwrap();
            state.apply(ClientEvent::Snapshot(Arc::new(next.clone())));
            state.apply(ClientEvent::Surface(Arc::new(frame)));
        }
        project_until(
            view,
            cx,
            "hovered pane frame before acknowledgement",
            |view| view.live.snapshot.as_ref().unwrap().revision == next.revision,
        );
        assert!(!view.input_ready());
    });
    // The real response travels through the socket worker and its inbox.
    let id = barrier["id"].as_str().unwrap();
    write_message(
        &mut server.stream,
        &ServerMessage::ClientShellEndpointResponseChunk {
            boot_id: next.boot_id.clone(),
            request_id: id.into(),
            final_chunk: true,
            data: serde_json::to_vec(&serde_json::json!({"id": id, "result": {
                "type": "client_shell_surface_set", "active": true,
                "projection_revision": next.revision,
            }}))
            .unwrap(),
        },
        MAX_GRAPHICS_FRAME_SIZE,
    )
    .unwrap();
    view.update(cx, |view, cx| {
        project_until(
            view,
            cx,
            "hover focus completion",
            SheprdWindow::input_ready,
        );
        assert!(!view.live.activation_pending());
        assert!(view.activation_deadline.is_none());
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    cx.simulate_input("a");
    assert_eq!(
        past_resizes(&mut server),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p2".into(),
            events: vec![ClientPaneInputEvent::TextCommit("a".into())],
        }
    );
}
