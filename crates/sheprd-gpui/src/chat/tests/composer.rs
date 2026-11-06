//! The composer keeps a prompt's lines: a pasted or recalled prompt with
//! several lines goes into the draft line by line, and a long draft never
//! pushes the editable line out of the pane.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::{ClipboardItem, TestAppContext};
use sheprd_chat_protocol::Command;
use sheprd_client::ConnectTarget;

fn idle() -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![agent(1, "Hi.")];
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

fn prompt_text(stream: &mut Peer) -> String {
    match read(stream) {
        Some(ClientMessage::Command {
            command: Command::Prompt { text, .. },
            ..
        }) => text,
        other => panic!("a prompt, got {other:?}"),
    }
}

fn paste_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-v"
    } else {
        "ctrl-shift-v"
    }
}

#[gpui::test]
fn a_pasted_prompt_keeps_its_lines(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, idle(), cx);
    cx.simulate_input("see: ");
    cx.write_to_clipboard(ClipboardItem::new_string(
        "fn main() {\r\n    run();\n}".into(),
    ));
    cx.simulate_keystrokes(paste_key());
    poll(&view, &program, cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(prompt_text(&mut stream), "see: fn main() {\n    run();\n}");

    // Up brings the whole prompt back, lines and all.
    cx.simulate_keystrokes("up");
    cx.simulate_keystrokes("enter");
    assert_eq!(prompt_text(&mut stream), "see: fn main() {\n    run();\n}");
}

#[gpui::test]
fn a_long_prompt_grows_the_field_and_keeps_it_in_the_pane(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, idle(), cx);
    poll(&view, &program, cx);
    let single = cx.debug_bounds("chat-composer").unwrap().size.height;
    let lines: Vec<String> = (0..40).map(|n| format!("line {n}")).collect();
    cx.write_to_clipboard(ClipboardItem::new_string(lines.join("\n")));
    cx.simulate_keystrokes(paste_key());
    poll(&view, &program, cx);
    let grown = cx.debug_bounds("chat-composer").unwrap().size.height;
    assert!(
        grown > single,
        "the field grows with the text: {single:?} -> {grown:?}"
    );
    let chat = cx.debug_bounds("chat-view").unwrap();
    let composer = cx.debug_bounds("chat-composer").unwrap();
    assert!(
        composer.bottom() <= chat.bottom(),
        "{composer:?} in {chat:?}"
    );
}

#[gpui::test]
fn a_paste_splits_at_the_cursor(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, idle(), cx);
    cx.simulate_input("before");
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("AB");
    cx.simulate_keystrokes("left");
    cx.write_to_clipboard(ClipboardItem::new_string("x\ny".into()));
    cx.simulate_keystrokes(paste_key());
    poll(&view, &program, cx);
    // Typing goes on right after the pasted text, before the line's tail.
    cx.simulate_input("Z");
    cx.simulate_keystrokes("enter");
    assert_eq!(prompt_text(&mut stream), "before\nAx\nyZB");
}

#[gpui::test]
fn a_prompt_waits_for_its_files_to_finish_copying(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, idle(), cx);
    view.update(cx, |view, _| {
        if let Some(pane) = view.chats.panes_mut().next() {
            pane.uploading = 1;
        }
    });
    cx.simulate_input("look at this");
    cx.simulate_keystrokes("enter");
    let error = view.read_with(cx, |view, _| {
        view.chats
            .panes()
            .next()
            .and_then(|pane| pane.error.clone())
    });
    assert!(error.is_some(), "the send is held while files copy");
    view.update(cx, |view, _| {
        if let Some(pane) = view.chats.panes_mut().next() {
            pane.uploading = 0;
        }
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(prompt_text(&mut stream), "look at this");
}

#[gpui::test]
fn a_long_queue_keeps_the_composer_in_the_pane(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.queued = (0..20)
        .map(|n| sheprd_chat_protocol::Queued {
            attachments: Vec::new(),
            id: sheprd_chat_protocol::CommandId::random(),
            text: format!("queued {n}"),
        })
        .collect();
    let snapshot = ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    };
    let _stream = attach_with(&view, &program, snapshot, cx);
    let queue = cx.debug_bounds("chat-queue").unwrap();
    let chat = cx.debug_bounds("chat-view").unwrap();
    assert!(
        queue.size.height < chat.size.height / 2.0,
        "the queue is bounded"
    );
    // Every queued prompt keeps its withdraw button, in the scrolled list.
    assert!(cx.debug_bounds("chat-unqueue-19").is_some());
    let composer = cx.debug_bounds("chat-composer").unwrap();
    assert!(
        composer.bottom() <= chat.bottom(),
        "{composer:?} in {chat:?}"
    );
}

#[gpui::test]
fn shift_enter_writes_a_newline_and_enter_sends_both(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, idle(), cx);
    cx.simulate_input("first");
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("second");
    cx.simulate_keystrokes("enter");
    assert_eq!(prompt_text(&mut stream), "first\nsecond");
}

#[gpui::test]
fn a_long_line_wraps_onto_more_rows_while_typing(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, idle(), cx);
    poll(&view, &program, cx);
    let single = cx.debug_bounds("chat-composer").unwrap().size.height;
    cx.simulate_input(&"one two three four five six seven eight nine ten ".repeat(6));
    poll(&view, &program, cx);
    let wrapped = cx.debug_bounds("chat-composer").unwrap().size.height;
    assert!(
        wrapped > single,
        "the field grows as the text wraps: {single:?} -> {wrapped:?}"
    );
}
