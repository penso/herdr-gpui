//! Every layout shows, under the conversation, that the agent is still on
//! a turn, and stops once the turn ends.

use super::{
    overlay::{attach_with, open, poll},
    timeline::conversation,
    *,
};
use crate::config::ChatLayoutMode;
use gpui::{Modifiers, TestAppContext};
use sheprd_chat_protocol::{Tool, ToolKind, ToolStatus};
use sheprd_client::ConnectTarget;
use std::time::Instant;

fn running() -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.entries = vec![Entry {
        id: EntryId(1),
        kind: EntryKind::User {
            text: "Fix it".into(),
            files: 0,
        },
    }];
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

#[gpui::test]
fn every_layout_shows_the_agent_working(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, running(), cx);
    for &layout in ChatLayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(layout, |_| Ok(()), cx)
        });
        poll(&view, &program, cx);
        // iMessage says it its own way, as Messages does.
        let sign = if layout == ChatLayoutMode::IMessage {
            "chat-typing"
        } else {
            "chat-working"
        };
        let working = cx
            .debug_bounds(sign)
            .unwrap_or_else(|| panic!("{layout:?}"));
        let composer = cx.debug_bounds("chat-composer").unwrap();
        assert!(
            working.bottom() <= composer.top(),
            "{layout:?}: above the composer"
        );
    }
}

#[gpui::test]
fn the_sign_is_gone_between_turns(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, conversation(false), cx);
    for &layout in ChatLayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(layout, |_| Ok(()), cx)
        });
        poll(&view, &program, cx);
        assert!(cx.debug_bounds("chat-working").is_none(), "{layout:?}");
        assert!(cx.debug_bounds("chat-typing").is_none(), "{layout:?}");
    }
}

fn running_step() -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.entries = vec![Entry {
        id: EntryId(1),
        kind: EntryKind::Tool(Tool {
            call_id: "call".into(),
            content: Vec::new(),
            kind: ToolKind::Execute,
            locations: Vec::new(),
            parent: None,
            started_ms: None,
            status: ToolStatus::InProgress,
            title: "sed -i one\nsed -i two".into(),
        }),
    }];
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

#[gpui::test]
fn the_row_opens_and_closes_the_steps_in_full(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, running_step(), cx);
    assert!(cx.debug_bounds("chat-steps").is_none());
    let row = cx.debug_bounds("chat-working").unwrap();
    cx.simulate_click(row.center(), Modifiers::default());
    // Once it has grown all the way in: the time is read before the frame
    // is drawn, so the frame checked is one drawn after the growth ended.
    let opened = Instant::now();
    wait_until(|| {
        let grown = opened.elapsed() > crate::motion::ENTER * 2;
        poll(&view, &program, cx);
        grown && cx.debug_bounds("chat-step-0").is_some()
    });
    let step = cx.debug_bounds("chat-step-0").unwrap();
    assert!(
        step.size.height > row.size.height,
        "both lines of the command show"
    );
    let composer = cx.debug_bounds("chat-composer").unwrap();
    assert!(step.bottom() <= composer.top());
    let row = cx.debug_bounds("chat-working").unwrap();
    cx.simulate_click(row.center(), Modifiers::default());
    wait_until(|| {
        poll(&view, &program, cx);
        cx.debug_bounds("chat-steps").is_none()
    });
}
