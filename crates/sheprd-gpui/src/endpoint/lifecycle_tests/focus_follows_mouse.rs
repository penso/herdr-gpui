use super::*;

#[gpui::test]
fn focus_follows_mouse_uses_one_ordered_focus_fence(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:hover");
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            window.focus(&view.focus, cx);
            assert!(window.is_window_active());
            view.config.focus_follows_mouse = true;
            // Ordinary shells must work without opting into terminal mouse reporting.
            for pane in &mut Arc::make_mut(view.live.surface.as_mut().unwrap()).panes {
                pane.mouse_reporting = false;
            }
            let mut event = MouseMoveEvent {
                position: mouse_position(view, 3.5, 4.5),
                ..Default::default()
            };
            view.focus_hovered_pane(&event, window, cx);
            assert!(view.input_ready());
            event.position = mouse_position(view, 43.5, 4.5);
            view.focus_hovered_pane(&event, window, cx);
            assert!(!view.input_ready());
            assert!(view.live.activation.is_some());
            for _ in 0..10 {
                view.focus_hovered_pane(&event, window, cx);
            }
            view.send(ClientPaneInputEvent::TextCommit("fenced".into()), cx);
            view.endpoints[1]
                .connection
                .handle
                .as_ref()
                .unwrap()
                .set_focus(&snapshot().boot_id, false)
                .unwrap();
        });
    });
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing hover focus");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "pane.focus");
    assert_eq!(request["params"], serde_json::json!({"pane_id": "w1:p2"}));
    server.respond(&request);
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing surface fence");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], Method::ClientShellSurfaceSet.as_str());
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
}

#[gpui::test]
fn focus_follows_mouse_respects_input_ownership(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:hover-guards");
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint, cx);
            window.focus(&view.focus, cx);
            let mut event = MouseMoveEvent {
                position: mouse_position(view, 43.5, 4.5),
                ..Default::default()
            };
            // Explicitly disabling the preference keeps click-to-focus.
            view.config.focus_follows_mouse = false;
            view.focus_hovered_pane(&event, window, cx);
            view.config.focus_follows_mouse = true;
            for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
                event.pressed_button = Some(button);
                view.focus_hovered_pane(&event, window, cx);
            }
            event.pressed_button = None;
            event.modifiers.shift = true;
            view.focus_hovered_pane(&event, window, cx);
            event.modifiers.shift = false;
            view.marked = "composition".into();
            view.focus_hovered_pane(&event, window, cx);
            view.marked.clear();
            let other = cx.focus_handle();
            window.focus(&other, cx);
            view.focus_hovered_pane(&event, window, cx);
            window.focus(&view.focus, cx);
            event.position = mouse_position(view, 90., 4.5);
            view.focus_hovered_pane(&event, window, cx);
            event.position = mouse_position(view, 43.5, 4.5);
            view.menu.page = Some(crate::menu::Page::Workspace);
            view.focus_hovered_pane(&event, window, cx);
            view.menu.page = None;
            let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
            surface.popup = Some(Box::new(ClientShellPopupSurface {
                terminal_id: "popup".into(),
                title: String::new(),
                width: None,
                height: None,
                frame: FrameData {
                    width: 10,
                    height: 10,
                    ..surface.frame.clone()
                },
                mouse_reporting: false,
                sgr_pixel_mouse: false,
                pixel_width: 100,
                pixel_height: 100,
            }));
            view.focus_hovered_pane(&event, window, cx);
            assert!(view.input_ready());
            assert!(view.live.activation.is_none());
            Arc::make_mut(view.live.surface.as_mut().unwrap()).popup = None;
        });
    });
    cx.deactivate_window();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(!window.is_window_active());
            let event = MouseMoveEvent {
                position: mouse_position(view, 43.5, 4.5),
                ..Default::default()
            };
            view.focus_hovered_pane(&event, window, cx);
            assert!(view.live.activation.is_none());
            view.endpoints[1]
                .connection
                .handle
                .as_ref()
                .unwrap()
                .set_focus(&snapshot().boot_id, false)
                .unwrap();
        });
    });
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
}
