//! Real GPUI events against an isolated chat peer: controls only edit local
//! values, and an explicit action uses the ordinary prompt path.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::chat::prepared as prepared_body;
use gpui::{ClipboardItem, Modifiers, TestAppContext, VisualTestContext};
use herdr_chat_protocol::Command;
use herdr_client::ConnectTarget;

fn card(json: &str) -> String {
    format!("```herdr-ui\n{json}\n```")
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::default());
}

#[gpui::test]
fn choosing_a_value_does_not_send_until_the_action_is_clicked(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let body = card(
        r#"{"components":[
      {"type":"choice","id":"view","label":"View","options":["Summary","Details"]},
      {"type":"action","label":"Discuss","prompt":"Explain the chosen view","mode":"send"}
    ]}"#,
    );
    let mut stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, &body)]),
        cx,
    );
    click(cx, "widget-w1:p1-1-0-0-1");
    poll(&view, &program, cx);
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
    click(cx, "widget-w1:p1-1-0-1");
    let Some(ClientMessage::Command {
        command: Command::Prompt { text, attachments },
        ..
    }) = read(&mut stream)
    else {
        panic!("an ordinary prompt");
    };
    assert!(text.starts_with("Explain the chosen view\n\nSelected values:\n"));
    assert!(text.contains("\"view\": \"Details\""));
    assert!(attachments.is_empty());
}

#[gpui::test]
fn typing_and_enter_in_a_widget_preserve_the_composer_and_draft_action_appends(
    cx: &mut TestAppContext,
) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let body = card(
        r#"{"components":[
      {"type":"input","id":"note","label":"Note","placeholder":"Type here"},
      {"type":"action","label":"Review","prompt":"Review this note"}
    ]}"#,
    );
    let _stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, &body)]),
        cx,
    );
    view.update(cx, |view, cx| {
        let input = view.chats.pane("w1:p1").unwrap().composer.clone().unwrap();
        input.update(cx, |input, cx| input.set_text_at("Keep my draft", 13, cx));
    });
    poll(&view, &program, cx);
    click(cx, "widget-w1:p1-1-0-0");
    poll(&view, &program, cx);
    assert!(cx.update(|window, cx| {
        view.read(cx)
            .chats
            .pane("w1:p1")
            .unwrap()
            .widgets
            .focused(window, cx)
    }));
    cx.simulate_input("edited note");
    cx.simulate_keystrokes("enter");
    view.update(cx, |view, cx| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        assert_eq!(
            pane.composer.as_ref().unwrap().read(cx).text(),
            "Keep my draft"
        );
        assert!(pane.session.resend().is_empty());
    });
    poll(&view, &program, cx);
    click(cx, "widget-w1:p1-1-0-1");
    view.update(cx, |view, cx| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        let text = format!(
            "{}\n{}",
            pane.draft.join("\n"),
            pane.composer.as_ref().unwrap().read(cx).text()
        );
        assert!(text.starts_with("Keep my draft\n"));
        assert!(text.contains("\"note\": \"edited note\""));
        assert!(pane.session.resend().is_empty());
    });
}

#[gpui::test]
fn a_read_only_mirror_allows_local_choices_but_never_sends(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let body = card(
        r#"{"components":[
      {"type":"choice","id":"view","label":"View","options":["A","B"]},
      {"type":"action","label":"Continue","prompt":"Continue","mode":"send"}
    ]}"#,
    );
    let mut message = snapshot(cursor(1, 0), vec![agent(1, &body)]);
    if let ServerMessage::Snapshot { transcript, .. } = &mut message {
        transcript.read_only = true;
    }
    let _stream = attach_with(&view, &program, message, cx);
    click(cx, "widget-w1:p1-1-0-0-1");
    poll(&view, &program, cx);
    click(cx, "widget-w1:p1-1-0-1");
    view.update(cx, |view, _| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        let body = pane.prepared.body(EntryId(1)).unwrap();
        let prepared_body::Body::Message(parts) = body.as_ref() else {
            panic!("message");
        };
        let prepared_body::Part::Widget(document) = &parts[0] else {
            panic!("widget");
        };
        assert_eq!(
            pane.widgets.form((EntryId(1), 0), document).unwrap().values["view"],
            crate::chat::widgets::Value::Text("B".into())
        );
        assert!(pane.session.resend().is_empty());
    });
}

#[gpui::test]
fn widget_editing_does_not_capture_composer_history_or_draft_keys(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let body = card(r#"{"components":[{"type":"input","id":"note","label":"Note"}]}"#);
    let _stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, &body)]),
        cx,
    );
    view.update(cx, |view, _| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        pane.draft.push("Keep this line".into());
        pane.history
            .push("Previous prompt", pane.session.transcript());
    });
    // The draft adds composer height and moves the bottom-aligned message.
    // Read hit bounds only after drawing that changed geometry.
    poll(&view, &program, cx);
    click(cx, "widget-w1:p1-1-0-0");
    poll(&view, &program, cx);
    assert!(cx.update(|window, cx| {
        view.read(cx)
            .chats
            .pane("w1:p1")
            .unwrap()
            .widgets
            .focused(window, cx)
    }));
    cx.simulate_keystrokes("backspace up down tab");
    cx.simulate_input("field value");
    poll(&view, &program, cx);
    view.update(cx, |view, cx| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        assert_eq!(pane.draft, ["Keep this line"]);
        assert_eq!(pane.composer.as_ref().unwrap().read(cx).text(), "");
        let body = pane.prepared.body(EntryId(1)).unwrap();
        let prepared_body::Body::Message(parts) = body.as_ref() else {
            panic!("message");
        };
        let prepared_body::Part::Widget(document) = &parts[0] else {
            panic!("widget");
        };
        assert_eq!(
            pane.widgets.form((EntryId(1), 0), document).unwrap().values["note"],
            crate::chat::widgets::Value::Text("field value".into())
        );
        assert!(pane.session.resend().is_empty());
    });
}

#[gpui::test]
fn pasted_widget_text_removes_directional_controls_from_both_field_and_values(
    cx: &mut TestAppContext,
) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let body = card(r#"{"components":[{"type":"input","id":"note","label":"Note"}]}"#);
    let _stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, &body)]),
        cx,
    );
    click(cx, "widget-w1:p1-1-0-0");
    poll(&view, &program, cx);
    cx.write_to_clipboard(ClipboardItem::new_string(
        "left\u{202e}middle\u{2066}right\u{2069}".into(),
    ));
    cx.dispatch_action(crate::actions::Paste);
    poll(&view, &program, cx);
    view.update(cx, |view, cx| {
        let pane = view.chats.pane_mut("w1:p1").unwrap();
        let body = pane.prepared.body(EntryId(1)).unwrap();
        let prepared_body::Body::Message(parts) = body.as_ref() else {
            panic!("message")
        };
        let prepared_body::Part::Widget(document) = &parts[0] else {
            panic!("widget")
        };
        let form = pane.widgets.form((EntryId(1), 0), document).unwrap();
        assert_eq!(
            form.inputs["note"].entity.read(cx).text(),
            "leftmiddleright"
        );
        assert_eq!(
            form.values["note"],
            crate::chat::widgets::Value::Text("leftmiddleright".into())
        );
        assert!(pane.session.resend().is_empty());
    });
}

#[gpui::test]
fn an_unfinished_or_unknown_widget_stays_readable_source(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let body = "```herdr-ui\n{\"components\":[{\"type\":\"script\"}]}\n```\n\n```herdr-ui\n{";
    let _stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, body)]),
        cx,
    );
    assert!(cx.debug_bounds("chat-widget").is_none());
    assert!(cx.debug_bounds("chat-agent").is_some());
}
