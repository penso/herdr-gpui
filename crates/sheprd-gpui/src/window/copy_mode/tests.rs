use super::*;
// `super::*` brings in gpui's `test`, which `#[gpui::test]` expands to.
use crate::{controls::Command, sidebar::layout_tests::fixture_window, window::MockPeer};
use core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use sheprd_client::protocol::{ClientMessage, PaneSurfaceScrollMetrics};

/// The next scrollback or scroll request on the wire, answering others;
/// terminal input must never appear.
fn next_request(peer: &mut MockPeer) -> Value {
    loop {
        match peer.receive() {
            ClientMessage::ClientShellEndpointRequest { request, .. } => {
                let request: Value = serde_json::from_str(&request).unwrap();
                if request["method"]
                    .as_str()
                    .is_some_and(|method| method.starts_with("pane."))
                {
                    return request;
                }
                let id = request["id"].as_str().unwrap();
                peer.respond("boot-v1", id, &json!({"id": id, "result": {"type": "ok"}}));
            }
            message @ (ClientMessage::ClientShellPaneInput { .. }
            | ClientMessage::ClientShellPopupInput { .. }) => {
                panic!("copy mode typed into the terminal: {message:?}")
            }
            _ => {}
        }
    }
}

#[gpui::test]
fn copy_mode_walks_the_history_and_copies_without_typing(cx: &mut TestAppContext) {
    let mut peer =
        MockPeer::advertising(&["pane.copy_motion", "pane.selection.read", "pane.scroll"]);
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        peer.prepare(&mut view, cx);
        view.live.supports_copy_motion = true;
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
    assert!(cx.debug_bounds("copy-mode").is_some());
    let poll = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.poll_copy_mode(window, cx);
                view.follow_selection(cx);
            })
        })
    };
    let inbox = view.read_with(cx, |view, _| {
        view.endpoints[0].connection.scrollback.clone()
    });
    let answer = |peer: &mut MockPeer, request: &Value, result: Value| {
        let id = request["id"].as_str().unwrap();
        let event = peer.respond("boot-v1", id, &json!({"id": id, "result": result}));
        assert!(inbox.lock().unwrap().apply(event).is_none());
    };

    // Up a row locally, then a word motion goes to the daemon; a key
    // typed meanwhile waits for it. An unknown key types nothing.
    cx.simulate_keystrokes("k x w l");
    let motion = next_request(&mut peer);
    assert_eq!(motion["method"], "pane.copy_motion");
    assert_eq!(
        motion["params"],
        json!({"pane_id": "w1:p1", "cursor": {"row": 122, "col": 0},
            "motion": "next_word_start", "content_revision": 2})
    );
    answer(
        &mut peer,
        &motion,
        json!({"type": "pane_copy_motion", "pane_id": "w1:p1",
            "cursor": {"row": 122, "col": 4}, "content_revision": 2}),
    );
    poll(cx);
    // Text an input method commits runs as keys too: mark, then the top
    // of history, which scrolls the pane there.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            EntityInputHandler::replace_text_in_range(view, None, "vg", window, cx)
        })
    });
    let scroll = next_request(&mut peer);
    assert_eq!(scroll["method"], "pane.scroll");
    assert_eq!(scroll["params"]["offset_from_bottom"], 100);
    // A scroll is a plain request, answered outside the scrollback mailbox.
    let id = scroll["id"].as_str().unwrap();
    peer.respond("boot-v1", id, &json!({"id": id, "result": {"type": "ok"}}));

    // Yank reads the marked range, from the history to the queued key's
    // column, and leaves copy mode where the pane was.
    cx.simulate_keystrokes("y");
    let read = next_request(&mut peer);
    assert_eq!(read["method"], "pane.selection.read");
    assert_eq!(
        read["params"],
        json!({"pane_id": "w1:p1", "anchor": {"row": 0, "col": 0},
            "cursor": {"row": 122, "col": 5}})
    );
    view.read_with(cx, |view, _| assert!(view.copy_mode.is_none()));
    answer(
        &mut peer,
        &read,
        json!({"type": "pane_selection", "pane_id": "w1:p1", "text": "copied lines"}),
    );
    // The scroll back to where copy mode began follows the read.
    let back = next_request(&mut peer);
    assert_eq!(back["method"], "pane.scroll");
    assert_eq!(back["params"]["offset_from_bottom"], 0);
    poll(cx);
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
        Some("copied lines".into())
    );
}

#[gpui::test]
fn copy_mode_explains_an_old_daemon(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::CopyMode, window, cx);
            assert!(view.copy_mode.is_none());
            assert!(view.flash.is_some());
        })
    });
}

mod padding;
mod search;
