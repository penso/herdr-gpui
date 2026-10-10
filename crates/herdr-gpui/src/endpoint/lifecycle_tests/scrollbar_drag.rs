use super::*;

/// The thumb stays held while a snapshot lands ahead of its surface, as one
/// does whenever a busy session changes, and follows the pointer once the
/// surface catches up.
#[gpui::test]
fn scrollbar_drag_survives_a_snapshot_ahead_of_its_surface(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:scrollbar");
    let drag = |view: &mut HerdrWindow, row, cx: &mut Context<HerdrWindow>| {
        assert!(view.scrollbar_mouse_move(
            &MouseMoveEvent {
                position: mouse_position(view, 38.5, row),
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            },
            cx
        ));
    };
    // A busy inbox defers a send to the next tick; tick until it goes.
    let sent = |view: &mut HerdrWindow, cx: &mut Context<HerdrWindow>| {
        wait_until(|| {
            view.flush_scrollbar(cx);
            view.live.drag_request.is_some()
        });
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            let pane = &mut Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0];
            pane.scrollbar_rect = Some(SurfaceRect {
                x: 38,
                y: 1,
                width: 1,
                height: 22,
            });
            pane.scroll = Some(PaneSurfaceScrollMetrics {
                offset_from_bottom: 0,
                max_offset_from_bottom: 1000,
                viewport_rows: 22,
            });
            // Projections of later answers must keep presenting this bar.
            let surface = view.live.surface.clone().unwrap();
            view.endpoints[1]
                .connection
                .inbox
                .lock()
                .unwrap()
                .apply(ClientEvent::Surface(surface));
            // Grab the thumb, at rest at the bottom of the track.
            assert!(view.scrollbar_mouse_down(
                &MouseDownEvent {
                    position: mouse_position(view, 38.5, 22.5),
                    button: MouseButton::Left,
                    ..Default::default()
                },
                cx,
            ));
            sent(view, cx);
        });
    });
    let scroll = |server: &mut Server| {
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing scroll request");
        };
        let request: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], "pane.scroll");
        assert_eq!(request["params"]["pane_id"], "w1:p1");
        server.respond(&request);
        request["params"]["offset_from_bottom"].as_u64().unwrap()
    };
    // The press asks for where the thumb already is.
    assert_eq!(scroll(&mut server), 0);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            project_until(view, cx, "press answer", |view| {
                view.live.drag_request.is_none()
            });
            let surface = view.live.surface.take();
            drag(view, 12., cx);
            assert!(view.scrollbar_drag.is_some());
            assert!(view.live.drag_request.is_none());
            view.live.surface = surface;
            drag(view, 12., cx);
            sent(view, cx);
        });
    });
    assert!(scroll(&mut server) > 0);
}
