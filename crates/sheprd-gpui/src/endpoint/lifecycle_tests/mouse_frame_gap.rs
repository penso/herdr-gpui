use super::*;

/// Puts the frame out of step with the snapshot (`"revision"`) or sizes it for
/// another client (`"size"`), or puts it back.
fn open_gap(view: &mut SheprdWindow, gap: &str, open: bool) {
    let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
    match (gap, open) {
        ("revision", true) => surface.projection_revision -= 1,
        ("revision", false) => surface.projection_revision += 1,
        ("size", true) => surface.frame.width -= 1,
        ("size", false) => surface.frame.width += 1,
        _ => unreachable!(),
    }
}

fn drag(view: &mut SheprdWindow, column: f32, row: f32, cx: &mut Context<SheprdWindow>) -> bool {
    view.terminal_mouse_move(
        &MouseMoveEvent {
            position: mouse_position(view, column, row),
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        },
        cx,
    )
}

fn press(
    view: &mut SheprdWindow,
    column: f32,
    row: f32,
    window: &mut gpui::Window,
    cx: &mut Context<SheprdWindow>,
) -> bool {
    view.terminal_mouse_down(
        &MouseDownEvent {
            position: mouse_position(view, column, row),
            button: MouseButton::Left,
            ..Default::default()
        },
        window,
        cx,
    )
}

fn release(view: &mut SheprdWindow, column: f32, row: f32, cx: &mut Context<SheprdWindow>) -> bool {
    view.terminal_mouse_up(
        &MouseUpEvent {
            position: mouse_position(view, column, row),
            button: MouseButton::Left,
            ..Default::default()
        },
        cx,
    )
}

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
                assert!(press(view, 3.5, 4.5, window, cx));
                assert!(drag(view, 5.5, 6.5, cx));
                open_gap(view, gap, true);
                assert!(!view.input_ready(), "{gap}");
                assert!(drag(view, 5.5, 6.5, cx), "{gap}");
                assert!(view.terminal_mouse.is_some(), "{gap}");
                open_gap(view, gap, false);
                assert!(view.input_ready(), "{gap}");
                assert!(drag(view, 7.5, 8.5, cx), "{gap}");
                assert!(release(view, 7.5, 8.5, cx), "{gap}");
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

/// A release while the gap lasts ends the drag where the application last saw
/// it, once. A drag left open would keep the application selecting.
#[gpui::test]
fn connected_mouse_release_during_a_frame_gap_ends_the_drag_once(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for gap in ["revision", "size"] {
        let (endpoint, mut server) = connected_endpoint("ssh:mouse");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                prepare_mouse(view, endpoint, cx);
                assert!(press(view, 3.5, 4.5, window, cx));
                assert!(drag(view, 5.5, 6.5, cx));
                open_gap(view, gap, true);
                assert!(drag(view, 7.5, 8.5, cx), "{gap}");
                assert!(release(view, 7.5, 8.5, cx), "{gap}");
                assert!(view.terminal_mouse.is_none(), "{gap}");
                // A later press shows that nothing else was sent in between.
                open_gap(view, gap, false);
                assert!(press(view, 3.5, 4.5, window, cx), "{gap}");
            });
        });
        for event in [
            mouse_event(ClientMouseKind::Down(ClientMouseButton::Left), 2, 3),
            mouse_event(ClientMouseKind::Drag(ClientMouseButton::Left), 4, 5),
            mouse_event(ClientMouseKind::Up(ClientMouseButton::Left), 4, 5),
            mouse_event(ClientMouseKind::Down(ClientMouseButton::Left), 2, 3),
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
