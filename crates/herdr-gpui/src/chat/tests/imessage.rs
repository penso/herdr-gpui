//! The iMessage layout: blue bubbles on the right for the user, gray ones
//! on the left for the agent, "Delivered" under an unanswered prompt, and
//! a round composer with attach before it and send inside it.

use super::{
    overlay::{attach_with, open, poll},
    timeline::conversation,
    *,
};
use crate::{HerdrWindow, config::ChatLayoutMode};
use gpui::{Entity, TestAppContext, VisualTestContext};
use herdr_client::ConnectTarget;

mod copy;

fn imessage(view: &Entity<HerdrWindow>, program: &Program, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.set_chat_layout_with(ChatLayoutMode::IMessage, |_| Ok(()), cx)
    });
    poll(view, program, cx);
}

#[gpui::test]
fn the_user_speaks_on_the_right_and_the_agent_on_the_left(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, conversation(false), cx);
    imessage(&view, &program, cx);
    let chat = cx.debug_bounds("chat-view").unwrap();
    let prompt = cx.debug_bounds("chat-imessage-1").unwrap();
    assert!(prompt.center().x > chat.center().x, "the user on the right");
    // The agent's words and its tool call take bubbles too, on the left.
    for id in ["chat-imessage-2", "chat-imessage-3", "chat-imessage-4"] {
        let reply = cx.debug_bounds(id).unwrap();
        assert!(reply.center().x < chat.center().x, "{id} on the left");
    }
    // A Markdown reply keeps its words on one line rather than folding to
    // a word per line, and its copy button floats inside its bubble.
    let reply = cx.debug_bounds("chat-imessage-2").unwrap();
    assert!(reply.size.width > reply.size.height, "{reply:?}");
    let copy = cx.debug_bounds("chat-copy-2").unwrap();
    assert!(copy.left() >= reply.left() && copy.right() <= reply.right());
    assert!(
        cx.debug_bounds("chat-copy-footer-2").is_none(),
        "plain prose stays compact"
    );
    // The prompt was answered.
    assert!(cx.debug_bounds("chat-delivered").is_none());
}

#[gpui::test]
fn an_unanswered_prompt_reads_delivered(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![
        agent(1, "Hello."),
        Entry {
            id: EntryId(2),
            kind: EntryKind::User {
                text: "Fix it".into(),
                files: 0,
            },
        },
    ];
    let snapshot = ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    };
    let _stream = attach_with(&view, &program, snapshot, cx);
    imessage(&view, &program, cx);
    let prompt = cx.debug_bounds("chat-imessage-2").unwrap();
    let delivered = cx.debug_bounds("chat-delivered").unwrap();
    assert!(delivered.top() >= prompt.bottom(), "under the prompt");
    assert!(delivered.right() > prompt.center().x, "on the user's side");
}

#[gpui::test]
fn attach_sits_before_the_field_and_send_inside_it(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, conversation(false), cx);
    imessage(&view, &program, cx);
    let draft = cx.debug_bounds("chat-composer").unwrap();
    let attach = cx.debug_bounds("chat-attach").unwrap();
    let send = cx.debug_bounds("chat-send").unwrap();
    let settings = cx.debug_bounds("chat-settings").unwrap();
    assert!(attach.right() <= draft.left(), "attach before the draft");
    assert!(send.left() >= draft.right(), "send after the draft");
    assert!(settings.top() > draft.bottom(), "settings under the field");
}

#[gpui::test]
fn typing_shows_while_the_agent_works(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.entries = vec![Entry {
        id: EntryId(1),
        kind: EntryKind::User {
            text: "Fix it".into(),
            files: 0,
        },
    }];
    let snapshot = ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    };
    let _stream = attach_with(&view, &program, snapshot, cx);
    // Other layouts leave it to the header.
    assert!(cx.debug_bounds("chat-typing").is_none());
    imessage(&view, &program, cx);
    let prompt = cx.debug_bounds("chat-imessage-1").unwrap();
    let typing = cx.debug_bounds("chat-typing").unwrap();
    assert!(typing.top() >= prompt.bottom(), "under the conversation");
    let composer = cx.debug_bounds("chat-composer").unwrap();
    assert!(typing.bottom() <= composer.top(), "above the composer");
}

#[gpui::test]
fn typing_is_gone_once_the_turn_ends(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, conversation(false), cx);
    imessage(&view, &program, cx);
    assert!(cx.debug_bounds("chat-typing").is_none());
}

#[gpui::test]
fn a_streaming_reply_takes_the_whole_width_until_it_ends(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.entries = vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                text: "Fix it".into(),
                files: 0,
            },
        },
        agent(2, "On it."),
    ];
    let snapshot = ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    };
    let mut stream = attach_with(&view, &program, snapshot, cx);
    imessage(&view, &program, cx);
    let streaming = cx.debug_bounds("chat-imessage-2").unwrap();
    send(
        &mut stream,
        &change(
            Change::Status {
                status: Status::Idle,
            },
            cursor(1, 1),
        ),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            let chat = view.chats.pane("w1:p1").unwrap();
            chat.session.transcript().status == Status::Idle
        })
    });
    let done = cx.debug_bounds("chat-imessage-2").unwrap();
    assert!(
        streaming.size.width > done.size.width * 2.,
        "{streaming:?} then {done:?}"
    );
}
