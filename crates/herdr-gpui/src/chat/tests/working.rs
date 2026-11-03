//! Every layout shows, under the conversation, that the agent is still on
//! a turn, and stops once the turn ends.

use super::{
    overlay::{attach_with, open, poll},
    timeline::conversation,
    *,
};
use crate::config::ChatLayoutMode;
use gpui::TestAppContext;
use herdr_client::ConnectTarget;

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
