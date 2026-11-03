use super::*;
// `super::*` brings in gpui's `test`, which `#[gpui::test]` expands to.
use core::prelude::v1::test;

/// `/` and `?` open a native prompt that takes the typing; Escape closes
/// it without leaving copy mode, Enter searches, `n` and `N` repeat, and
/// `y` copies the match the cursor moved to. Nothing reaches the pane.
#[gpui::test]
fn copy_mode_searches_from_a_prompt_and_copies_the_match(cx: &mut TestAppContext) {
    let mut peer = MockPeer::advertising(&[
        "pane.copy_motion",
        "pane.copy_search",
        "pane.selection.read",
        "pane.scroll",
    ]);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        peer.prepare(&mut view, cx);
        view.live.supports_copy_motion = true;
        view.live.supports_copy_search = true;
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        surface.panes[0].content_revision = 2;
        surface.panes[0].scroll = Some(PaneSurfaceScrollMetrics {
            offset_from_bottom: 0,
            max_offset_from_bottom: 100,
            viewport_rows: 24,
        });
        view
    });
    let redraw = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
    };
    redraw(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::CopyMode, window, cx)));
    redraw(cx);
    let prompt_focused = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| view.read(cx).copy_search_focused(window, cx))
    };
    let inbox = view.read_with(cx, |view, _| {
        view.endpoints[0].connection.scrollback.clone()
    });
    let answer =
        |peer: &mut MockPeer, cx: &mut VisualTestContext, request: &Value, result: Value| {
            let id = request["id"].as_str().unwrap();
            let event = peer.respond("boot-v1", id, &json!({"id": id, "result": result}));
            assert!(inbox.lock().unwrap().apply(event).is_none());
            cx.update(|window, cx| view.update(cx, |view, cx| view.poll_copy_mode(window, cx)));
        };

    // Escape closes the prompt, and only the prompt.
    cx.simulate_keystrokes("/");
    redraw(cx);
    assert!(cx.debug_bounds("copy-search").is_some());
    assert!(prompt_focused(cx));
    cx.simulate_input("q");
    cx.simulate_keystrokes("escape");
    redraw(cx);
    assert!(!prompt_focused(cx));
    assert!(cx.debug_bounds("copy-search").is_none());
    view.read_with(cx, |view, _| {
        assert!(view.copy_mode.is_some(), "the q was the prompt's")
    });

    // `?` searches toward older output from the cursor, query as typed.
    cx.simulate_keystrokes("?");
    redraw(cx);
    cx.simulate_input("Err");
    cx.simulate_keystrokes("enter");
    let first = next_request(&mut peer);
    assert_eq!(first["method"], "pane.copy_search");
    assert_eq!(
        first["params"],
        json!({"pane_id": "w1:p1", "query": "Err", "direction": "backward",
            "cursor": {"row": 123, "col": 0}, "content_revision": 2})
    );
    assert!(!prompt_focused(cx));
    let found = |current: u32| {
        json!({"type": "pane_copy_search", "pane_id": "w1:p1", "content_revision": 2,
            "matches": [
                {"start": {"row": 110, "col": 3}, "end": {"row": 110, "col": 5}},
                {"start": {"row": 120, "col": 0}, "end": {"row": 120, "col": 2}},
            ],
            "total": 2, "current": current, "current_global": current})
    };
    answer(&mut peer, cx, &first, found(1));
    let highlights = view.read_with(cx, |view, _| {
        view.copy_mode_highlights(view.live.surface.as_deref().unwrap())
    });
    assert_eq!(highlights.len(), 3, "two matches and the cursor");

    // `n` goes on the way the query was entered, past the current match.
    cx.simulate_keystrokes("n");
    let repeat = next_request(&mut peer);
    assert_eq!(repeat["params"]["direction"], "backward");
    assert_eq!(
        repeat["params"]["previous"],
        json!({"start": {"row": 120, "col": 0}, "end": {"row": 120, "col": 2}})
    );
    answer(&mut peer, cx, &repeat, found(0));

    // `y` with nothing marked copies the match under the cursor, at the
    // revision it was found in.
    cx.simulate_keystrokes("y");
    let read = next_request(&mut peer);
    assert_eq!(read["method"], "pane.selection.read");
    assert_eq!(
        read["params"],
        json!({"pane_id": "w1:p1", "anchor": {"row": 110, "col": 3},
            "cursor": {"row": 110, "col": 5}, "content_revision": 2})
    );
    view.read_with(cx, |view, _| assert!(view.copy_mode.is_none()));
}

#[gpui::test]
fn copy_mode_search_explains_an_old_daemon(cx: &mut TestAppContext) {
    let peer = MockPeer::advertising(&["pane.copy_motion", "pane.scroll"]);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        peer.prepare(&mut view, cx);
        view.live.supports_copy_motion = true;
        view
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::CopyMode, window, cx);
            assert!(view.copy_mode.is_some());
            view.copy_mode_text("/x", window, cx);
            assert!(view.copy_mode.as_ref().unwrap().prompt.is_none());
            assert!(view.flash.is_some());
            assert!(view.copy_mode.is_some(), "copy mode stays on");
        })
    });
}
