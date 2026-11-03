use super::*;

#[gpui::test]
fn successful_resize_reassert_waits_for_the_next_retry_interval(cx: &mut gpui::TestAppContext) {
    let (endpoint, mut server) = connected_endpoint("resize-reassert");
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.read_with(cx, |fixture, _| fixture.0.clone());
    let start = Instant::now();
    view.update(cx, |view, cx| {
        view.endpoints = vec![endpoint];
        view.options = ConnectOptions::default();
        view.reset_selected(cx);
        view.options.surface_size.cols += 1;
        view.last_queued_options = Some(view.options);
        view.pending_resize = None;
        // The window is in use, so it keeps the tab at its own size.
        view.last_input = Some(start);
        view.resize_at(start);
        let sent = start + Duration::from_secs(1);
        view.resize_at(sent);
        for tick in 1..30 {
            view.resize_at(sent + Duration::from_millis(16 * tick));
        }
        view.endpoints[0]
            .connection
            .handle
            .as_ref()
            .unwrap()
            .set_focus(&snapshot().boot_id, false)
            .unwrap();
    });
    assert!(matches!(
        server.receive(),
        ClientMessage::ClientShellResize { .. }
    ));
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing focus claim after resize");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "tab.focus");
    server.respond(&request);
    assert!(
        matches!(
            server.receive(),
            ClientMessage::ClientShellFocus { focused: false }
        ),
        "a delayed frame must not queue a resize/focus pair on every UI tick"
    );

    view.update(cx, |view, _| {
        view.resize_at(start + Duration::from_secs(2));
        let mut frame = (**view.live.surface.as_ref().unwrap()).clone();
        frame.frame.width = view.options.surface_size.cols;
        frame.frame.cells.resize(
            usize::from(frame.frame.width) * usize::from(frame.frame.height),
            CellData {
                symbol: " ".into(),
                fg: 0,
                bg: 0,
                modifier: 0,
                skip: false,
                hyperlink: None,
            },
        );
        view.live.surface = Some(Arc::new(frame));
        view.resize_at(start + Duration::from_secs(3));
        assert!(view.pending_resize.is_none());
        view.endpoints[0]
            .connection
            .handle
            .as_ref()
            .unwrap()
            .set_focus(&snapshot().boot_id, true)
            .unwrap();
    });
    assert!(matches!(
        server.receive(),
        ClientMessage::ClientShellResize { .. }
    ));
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing focus claim after retry interval");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "tab.focus");
    server.respond(&request);
    assert!(matches!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: true }
    ));
}
