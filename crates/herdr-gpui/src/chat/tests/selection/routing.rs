use super::*;
use crate::{
    chat::prepared::{Body, Part},
    connection::ConnectionBridge,
    terminal::InputTarget,
};
use gpui::ClipboardItem;
use herdr_client::protocol::ClientPaneInputEvent;

#[gpui::test]
fn copying_selected_transcript_never_sends_control_c_to_the_original_cli(cx: &mut TestAppContext) {
    let (view, mut peer, program, _stream, cx) = terminal_input::observed(cx);
    let from = position(&view, caret(&view, 0, 0, 0, 4, cx), cx);
    let to = position(&view, caret(&view, 0, 0, 0, 8, cx), cx);
    drag(cx, from, to);
    cx.simulate_keystrokes(copy_key());
    assert_eq!(clipboard(cx).as_deref(), Some("same"));
    view.read_with(cx, |view, _| {
        ConnectionBridge::send_input(
            view.endpoints[0].connection.handle.as_ref().unwrap(),
            &view.live.snapshot.as_ref().unwrap().boot_id,
            &InputTarget::Pane("w1:p1".into()),
            ClientPaneInputEvent::TextCommit("sentinel".into()),
        )
        .unwrap();
    });
    assert_eq!(
        terminal_input::next_input(&mut peer),
        [ClientPaneInputEvent::TextCommit("sentinel".into())]
    );
    view.update(cx, |view, _| {
        assert!(
            view.chats
                .pane_mut("w1:p1")
                .unwrap()
                .session
                .resend()
                .is_empty()
        )
    });

    // Returning to the original editor restores ordinary typing once.
    let editor = cx.debug_bounds("chat-terminal-input").unwrap();
    cx.simulate_click(editor.center(), Modifiers::default());
    poll(&view, &program, cx);
    cx.simulate_input("!");
    assert_eq!(
        terminal_input::next_input(&mut peer),
        [ClientPaneInputEvent::TextCommit("!".into())]
    );
}

#[gpui::test]
fn composer_and_widget_selection_keep_their_own_clipboard_and_text(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let body = "Transcript text.\n\n```herdr-ui\n{\"components\":[{\"type\":\"input\",\"id\":\"note\",\"label\":\"Note\"}]}\n```";
    let _stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, body)]),
        cx,
    );
    let from = position(&view, caret(&view, 0, 0, 0, 0, cx), cx);
    let to = position(&view, caret(&view, 0, 0, 0, "Transcript".len(), cx), cx);
    drag(cx, from, to);
    cx.simulate_keystrokes(copy_key());
    assert_eq!(clipboard(cx).as_deref(), Some("Transcript"));

    let composer = cx.debug_bounds("chat-composer").unwrap();
    cx.simulate_click(composer.center(), Modifiers::default());
    cx.simulate_input("composer café");
    cx.simulate_keystrokes(select_all_key());
    cx.simulate_keystrokes(copy_key());
    assert_eq!(clipboard(cx).as_deref(), Some("composer café"));

    let widget = cx.debug_bounds("widget-w1:p1-1-1-0").unwrap();
    cx.simulate_click(widget.center(), Modifiers::default());
    cx.simulate_input("widget 中文");
    cx.simulate_keystrokes(select_all_key());
    cx.simulate_keystrokes(copy_key());
    assert_eq!(clipboard(cx).as_deref(), Some("widget 中文"));
    poll(&view, &program, cx);
    view.update(cx, |view, cx| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        assert_eq!(
            pane.composer.as_ref().unwrap().read(cx).text(),
            "composer café"
        );
        let body = pane.prepared.body(EntryId(1)).unwrap();
        let Body::Message(parts) = body.as_ref() else {
            panic!("message")
        };
        let Part::Widget(document) = &parts[1] else {
            panic!("widget")
        };
        assert_eq!(
            pane.widgets.form((EntryId(1), 1), document).unwrap().inputs["note"]
                .entity
                .read(cx)
                .text(),
            "widget 中文"
        );
        assert!(pane.session.resend().is_empty());
    });
}

#[gpui::test]
fn changed_revision_and_replacement_session_clear_selection_and_reject_old_callbacks(
    cx: &mut TestAppContext,
) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, "Original text")]),
        cx,
    );
    let old = caret(&view, 0, 0, 0, 0, cx);
    let end = caret(&view, 0, 0, 0, "Original text".len(), cx);
    let from = position(&view, old, cx);
    let to = position(&view, end, cx);
    drag(cx, from, to);
    send(&mut stream, &change(append(1, " extended"), cursor(1, 1)));
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            view.chats
                .pane("w1:p1")
                .unwrap()
                .prepared
                .text()
                .segment(old.key)
                .is_none()
        })
    });
    assert!(view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().selection.is_empty()
    }));
    cx.write_to_clipboard(ClipboardItem::new_string("unchanged clipboard".into()));
    cx.simulate_keystrokes(copy_key());
    assert_eq!(clipboard(cx).as_deref(), Some("unchanged clipboard"));

    let current = caret(&view, 0, 0, 0, 0, cx);
    let end = caret(&view, 0, 0, 0, "Original text extended".len(), cx);
    let from = position(&view, current, cx);
    let to = position(&view, end, cx);
    drag(cx, from, to);
    send(
        &mut stream,
        &snapshot(cursor(2, 0), vec![agent(1, "Replacement text")]),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            view.chats
                .pane("w1:p1")
                .unwrap()
                .prepared
                .text()
                .segment(current.key)
                .is_none()
        })
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.press_chat_text("w1:p1", current, 1, false, window, cx);
            let pane = view.chats.pane("w1:p1").unwrap();
            assert!(
                pane.selection.is_empty(),
                "a delayed old-frame callback cannot select the new session"
            );
            assert!(pane.selection.text(pane.prepared.text()).is_none());
        });
    });
}

#[gpui::test]
fn dragging_a_notice_label_keeps_it_folded_and_select_all_excludes_hidden_source(
    cx: &mut TestAppContext,
) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let label = "Background task finished";
    let _stream = attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![Entry {
                id: EntryId(1),
                kind: EntryKind::Notice {
                    answers: Vec::new(),
                    details: Some(
                        "Hidden original record must not be copied until expanded".into(),
                    ),
                    level: herdr_chat_protocol::NoticeLevel::Info,
                    text: label.into(),
                },
            }],
        ),
        cx,
    );
    let from = position(&view, caret(&view, 0, 0, 0, 0, cx), cx);
    let to = position(
        &view,
        caret(&view, 0, 0, 0, "Background task".len(), cx),
        cx,
    );
    drag(cx, from, to);
    poll(&view, &program, cx);
    assert!(
        cx.debug_bounds("chat-notice-details-1").is_none(),
        "dragging the label must not toggle the card"
    );
    cx.simulate_keystrokes(copy_key());
    assert_eq!(clipboard(cx).as_deref(), Some("Background task"));
    cx.simulate_keystrokes(select_all_key());
    cx.simulate_keystrokes(copy_key());
    assert_eq!(
        clipboard(cx).as_deref(),
        Some(label),
        "hidden source is not part of visible Select All"
    );
    let at = position(&view, caret(&view, 0, 0, 0, 2, cx), cx);
    cx.simulate_click(at, Modifiers::default());
    poll(&view, &program, cx);
    assert!(
        cx.debug_bounds("chat-notice-details-1").is_some(),
        "a plain click still opens the card"
    );
}
