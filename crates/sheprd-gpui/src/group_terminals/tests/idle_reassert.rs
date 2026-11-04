use super::*;

#[test]
fn an_idle_parked_connection_leaves_a_size_another_client_overrode() {
    let mut peer = MockPeer::new();
    let snapshot = fixture_snapshot();
    let workspace = snapshot.focused_workspace_id.clone().unwrap();
    let tab = snapshot.focused_tab_id.clone().unwrap();
    let boot = snapshot.boot_id.clone();
    let mut ids = crate::browser::GroupIds::default();
    let mut parked = parked(
        ids.next(),
        (Scope::endpoint("local"), workspace),
        sheprd_client::ConnectTarget::Socket("/unused-parked.sock".into()),
        &peer,
        snapshot,
    );
    // The daemon projects a frame of another size than the one queued, but
    // the window has had no input: another client in use keeps the tab.
    parked.options.surface_size.cols = 100;
    parked.last_queued_options = Some(parked.options);
    let now = Instant::now();
    parked.steer(&tab, now, None);
    assert_eq!(
        peer.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
    parked.steer(
        &tab,
        now + RESIZE_REASSERT + Duration::from_millis(10),
        None,
    );
    parked.steer(&tab, now + RESIZE_REASSERT * 3, None);
    // Messages leave in order, so a resize would arrive before this.
    peer.client.handle.set_focus(&boot, true).unwrap();
    assert_eq!(
        peer.receive(),
        ClientMessage::ClientShellFocus { focused: true },
        "an idle connection does not re-claim the frame"
    );
}

/// Shows one mouse-reporting pane over `peer`, ready for input, and returns a
/// position inside it.
fn reporting_pane(view: &mut SheprdWindow, peer: &MockPeer) -> Point<Pixels> {
    let fixture = fixture_snapshot();
    let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
    let mut pane = fixture.panes[0].clone();
    pane.workspace_id = "w0".into();
    pane.tab_id = "t0".into();
    shown.boot_id = fixture.boot_id;
    shown.focused_pane_id = Some(pane.pane_id.clone());
    shown.panes = vec![pane];
    let mut projected = surface(&shown, &shown.panes[0].pane_id);
    view.live.snapshot = Some(Arc::new(shown));
    let frame = Arc::make_mut(&mut projected);
    frame.frame.width = view.options.surface_size.cols;
    frame.frame.height = view.options.surface_size.rows;
    frame.panes[0].mouse_reporting = true;
    view.live.surface = Some(projected);
    // A connection traded in has said hello with an active surface.
    let endpoint = &mut view.endpoints[0];
    let mut connection = ConnectionBridge::new(endpoint.connection.target.clone());
    connection.handle = Some(peer.client.handle.clone());
    endpoint.trade_connection(&mut connection, &mut view.live.clone());
    view.endpoints[0].live = view.live.clone();
    view.cell_width = 10.;
    let line = view.config.terminal.line_height();
    view.bounds = Bounds::new(point(px(0.), px(0.)), size(px(800.), px(24. * line)));
    assert!(view.input_ready(), "the fixture sends input");
    point(px(15.), px(line / 2.))
}

#[gpui::test]
fn a_hover_report_alone_is_not_use(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let mut peer = MockPeer::new();
    let position = view.update(cx, |view, cx| {
        let position = reporting_pane(view, &peer);
        view.terminal_mouse_hover(
            &MouseMoveEvent {
                position,
                ..Default::default()
            },
            cx,
        );
        assert!(
            view.last_input.is_none(),
            "a hover report a resting cursor triggers is not use"
        );
        position
    });
    assert!(
        matches!(peer.receive(), ClientMessage::ClientShellPaneInput { .. }),
        "the hover still reaches the pane's program"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let press = MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count: 1,
                first_mouse: false,
            };
            assert!(view.terminal_mouse_down(&press, window, cx));
            assert!(view.last_input.is_some(), "a press is use");
        })
    });
}

#[gpui::test]
fn a_pointer_that_did_not_move_sends_no_hover_report(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let mut peer = MockPeer::new();
    let (position, boot) = view.update(cx, |view, _| {
        let position = reporting_pane(view, &peer);
        let boot = view.live.snapshot.as_ref().unwrap().boot_id.clone();
        (position, boot)
    });
    let resting = MouseMoveEvent {
        position,
        ..Default::default()
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.pane_pointer_moved(&resting, window, cx);
            view.pane_pointer_moved(&resting, window, cx);
        })
    });
    assert!(
        matches!(peer.receive(), ClientMessage::ClientShellPaneInput { .. }),
        "the first move reports hover"
    );
    // Messages leave in order, so a second report would arrive before this.
    peer.client.handle.set_focus(&boot, true).unwrap();
    assert_eq!(
        peer.receive(),
        ClientMessage::ClientShellFocus { focused: true },
        "a move event for a pointer that did not move reports nothing"
    );
    let moved = MouseMoveEvent {
        position: position + point(px(10.), px(0.)),
        ..Default::default()
    };
    cx.update(|window, cx| view.update(cx, |view, cx| view.pane_pointer_moved(&moved, window, cx)));
    assert!(
        matches!(peer.receive(), ClientMessage::ClientShellPaneInput { .. }),
        "moving to another cell reports hover again"
    );
}
