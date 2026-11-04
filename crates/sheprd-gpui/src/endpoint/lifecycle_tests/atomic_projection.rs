use super::*;

#[gpui::test]
fn metadata_gaps_deliver_keys_wheel_and_click_without_waiting_for_surface(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("metadata-gap");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            let mut next = (**view.live.snapshot.as_ref().unwrap()).clone();
            next.revision += 1;
            next.agents[0].title = Some("new spinner metadata".into());
            view.endpoints[1]
                .connection
                .inbox
                .lock()
                .unwrap()
                .apply(ClientEvent::Snapshot(Arc::new(next)));
            project_until(view, cx, "staged metadata", |view| {
                view.live.pending_snapshot.is_some()
            });
            assert!(view.input_ready());
            assert_eq!(
                view.live.snapshot.as_ref().unwrap().revision,
                view.live.surface.as_ref().unwrap().projection_revision
            );
            view.send(ClientPaneInputEvent::TextCommit("immediate".into()), cx);
            let position = mouse_position(view, 3.5, 4.5);
            view.scroll_wheel(
                &gpui::ScrollWheelEvent {
                    position,
                    delta: gpui::ScrollDelta::Lines(point(
                        0.,
                        f32::from(crate::terminal::PLATFORM_NOTCH_LINES),
                    )),
                    modifiers: Default::default(),
                    touch_phase: gpui::TouchPhase::Moved,
                },
                window,
                cx,
            );
            assert!(view.terminal_mouse_down(
                &MouseDownEvent {
                    position,
                    button: MouseButton::Left,
                    ..Default::default()
                },
                window,
                cx
            ));
            assert!(view.terminal_mouse_up(
                &MouseUpEvent {
                    position,
                    button: MouseButton::Left,
                    ..Default::default()
                },
                cx
            ));
            assert!(view.live.pending_snapshot.is_some());
            assert_eq!(view.pending_input.len(), 0);
        });
    });
    // The pane reports the mouse, so a notch arrives as one report per line.
    let notch = ClientPaneInputEvent::Mouse {
        kind: ClientMouseKind::ScrollUp,
        position: ClientMousePosition::Cell { column: 2, row: 3 },
        geometry: None,
        modifiers: 0,
        lines: 1,
    };
    let lines = usize::from(crate::herdr_settings::DEFAULT_MOUSE_SCROLL_LINES.get());
    for events in [
        vec![ClientPaneInputEvent::TextCommit("immediate".into())],
        vec![notch; lines],
        vec![mouse_event(
            ClientMouseKind::Down(ClientMouseButton::Left),
            2,
            3,
        )],
        vec![mouse_event(
            ClientMouseKind::Up(ClientMouseButton::Left),
            2,
            3,
        )],
    ] {
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events,
            }
        );
    }
}
