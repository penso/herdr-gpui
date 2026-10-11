use super::*;

/// A held drag in a mouse-reporting pane outlives a frame that is briefly out
/// of step with the snapshot or sized for another client. Ending it there
/// sends the application an early release, which a selecting application
/// takes as the end of the selection.
#[gpui::test]
fn connected_mouse_drag_waits_through_a_frame_gap(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for gap in ["revision", "size"] {
        let (endpoint, mut server) = connected_endpoint("ssh:mouse");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                prepare_mouse(view, endpoint, cx);
                let drag = |view: &mut HerdrWindow, column, row, cx: &mut Context<HerdrWindow>| {
                    view.terminal_mouse_move(
                        &MouseMoveEvent {
                            position: mouse_position(view, column, row),
                            pressed_button: Some(MouseButton::Left),
                            ..Default::default()
                        },
                        cx,
                    )
                };
                assert!(view.terminal_mouse_down(
                    &MouseDownEvent {
                        position: mouse_position(view, 3.5, 4.5),
                        button: MouseButton::Left,
                        ..Default::default()
                    },
                    window,
                    cx
                ));
                assert!(drag(view, 5.5, 6.5, cx));
                let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
                match gap {
                    "revision" => surface.projection_revision -= 1,
                    "size" => surface.frame.width -= 1,
                    _ => unreachable!(),
                }
                assert!(!view.input_ready(), "{gap}");
                assert!(drag(view, 5.5, 6.5, cx), "{gap}");
                assert!(view.terminal_mouse.is_some(), "{gap}");
                let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
                match gap {
                    "revision" => surface.projection_revision += 1,
                    "size" => surface.frame.width += 1,
                    _ => unreachable!(),
                }
                assert!(view.input_ready(), "{gap}");
                assert!(drag(view, 7.5, 8.5, cx), "{gap}");
                assert!(view.terminal_mouse_up(
                    &MouseUpEvent {
                        position: mouse_position(view, 7.5, 8.5),
                        button: MouseButton::Left,
                        ..Default::default()
                    },
                    cx
                ));
            });
        });
        for event in [
            mouse_event(ClientMouseKind::Down(ClientMouseButton::Left), 2, 3),
            mouse_event(ClientMouseKind::Drag(ClientMouseButton::Left), 4, 5),
            mouse_event(ClientMouseKind::Drag(ClientMouseButton::Left), 6, 7),
            mouse_event(ClientMouseKind::Up(ClientMouseButton::Left), 6, 7),
        ] {
            assert_eq!(
                server.receive(),
                ClientMessage::ClientShellPaneInput {
                    pane_id: "w1:p1".into(),
                    events: vec![event],
                },
                "{gap}"
            );
        }
    }
}
