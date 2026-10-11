use super::*;

/// Like a terminal's button-event tracking, a held drag reports motion only
/// when the pointer reaches another cell. Every pixel of motion within one
/// cell used to repeat the last report to the application.
#[gpui::test]
fn connected_mouse_drag_reports_each_cell_once(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
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
            // Motion inside the pressed cell is not a drag yet.
            assert!(drag(view, 3.2, 4.8, cx));
            assert!(drag(view, 5.5, 6.5, cx));
            assert!(drag(view, 5.1, 6.9, cx));
            assert!(drag(view, 5.9, 6.2, cx));
            assert!(drag(view, 7.5, 8.5, cx));
            assert!(view.terminal_mouse.is_some());
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
            }
        );
    }
}
