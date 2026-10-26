//! Up and Down step through the session's earlier prompts, whichever client
//! sent them, including those the session had before the chat attached.

use super::{
    overlay::{attach_with, open},
    *,
};
use gpui::TestAppContext;
use herdr_chat_protocol::Command;
use herdr_client::ConnectTarget;

fn user(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            text: text.into(),
            files: 0,
        },
    }
}

fn earlier() -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![
        user(1, "first"),
        agent(2, "Done."),
        user(3, "second"),
        agent(4, "Done."),
    ];
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

#[gpui::test]
fn up_and_down_step_through_the_sessions_prompts(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, earlier(), cx);

    cx.simulate_keystrokes("up up");
    cx.simulate_keystrokes("enter");
    assert_eq!(prompt_text(&mut stream), "first");

    cx.simulate_keystrokes("up up up down");
    cx.simulate_keystrokes("enter");
    assert_eq!(prompt_text(&mut stream), "second");
}
