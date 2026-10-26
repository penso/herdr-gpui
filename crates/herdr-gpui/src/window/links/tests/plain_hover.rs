use super::*;

/// Asked nothing: no resolve waits on its delay or on the daemon.
fn idle(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, _| {
        view.links.delay.is_none() && view.links.resolving.is_none()
    })
}

#[gpui::test]
fn plain_hover_reads_nothing_unless_configured(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    let position = at(&view, cx, 2, 1);
    cx.simulate_mouse_move(position, None, Modifiers::default());
    resolve_after_delay(cx);
    assert!(idle(&view, cx));
    view.read_with(cx, |view, _| assert!(view.links.hover.is_none()));
}

/// With `highlight_links_on_hover`, a plain pointer is underlined over a
/// link the daemon resolves, asks once per cell it reaches, and never again
/// while it rests on a pane that redraws. Opening still takes the modifier.
#[gpui::test]
fn plain_hover_underlines_and_asks_once_per_cell(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    view.update(cx, |view, _| view.config.highlight_links_on_hover = true);
    let continuation = at(&view, cx, 2, 1);
    cx.simulate_mouse_move(continuation, None, Modifiers::default());
    resolve_after_delay(cx);
    let request = peer.request();
    assert_eq!(request["method"], "pane.link.resolve");
    assert_eq!(request["params"]["viewport_row"], 1);
    assert_eq!(request["params"]["col"], 2);
    answer(
        &mut peer,
        &view,
        cx,
        &request,
        json!({"type": "pane_link_resolved", "regions": [
            {"row": 0, "start_col": 0, "end_col": 19},
            {"row": 1, "start_col": 0, "end_col": 11},
        ]}),
    );
    view.read_with(cx, |view, _| {
        assert!(view.hovered_daemon_link().is_some());
        // Underlined, but no hand: a plain click does not open it.
        assert!(!view.terminal_link_hovered(continuation, Modifiers::default()));
        assert!(
            view.terminal_link_press(continuation, Modifiers::default())
                .is_none()
        );
    });

    // Moving along the link asks nothing new.
    let first_row = at(&view, cx, 5, 0);
    cx.simulate_mouse_move(first_row, None, Modifiers::default());
    cx.simulate_mouse_move(continuation, None, Modifiers::default());
    resolve_after_delay(cx);
    assert!(idle(&view, cx));

    // A redraw under the still pointer drops the underline it no longer
    // matches and asks nothing, however often it polls.
    view.update(cx, |view, _| view.live.surface = Some(wrapped(4)));
    for _ in 0..3 {
        cx.update(|window, cx| view.update(cx, |view, cx| view.poll_links(window, cx)));
        resolve_after_delay(cx);
    }
    assert!(idle(&view, cx));
    view.read_with(cx, |view, _| assert!(view.hovered_daemon_link().is_none()));

    // Reaching another cell asks about it, with the content now shown. The
    // next request being this one shows the redraw sent none.
    let next = at(&view, cx, 3, 1);
    cx.simulate_mouse_move(next, None, Modifiers::default());
    resolve_after_delay(cx);
    let request = peer.request();
    assert_eq!(request["method"], "pane.link.resolve");
    assert_eq!(request["params"]["content_revision"], 4);
    assert_eq!(request["params"]["col"], 3);
}
