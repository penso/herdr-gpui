//! The working row tells an agent waiting on the user from a busy one in
//! every layout, and a chat with no composer keeps its context figure.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::config::ChatLayoutMode;
use gpui::TestAppContext;
use sheprd_chat_protocol::{Permission, PermissionId, PermissionOption, Usage};
use sheprd_client::{ConnectTarget, protocol::AgentStatus};

fn blocked() -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.permissions = vec![Permission {
        call_id: None,
        id: PermissionId(1),
        options: vec![PermissionOption {
            id: "allow".into(),
            kind: Default::default(),
            name: "Allow".into(),
        }],
        title: "Run ls".into(),
    }];
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

#[gpui::test]
fn every_layout_stops_pulsing_while_the_agent_waits(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, blocked(), cx);
    for &layout in ChatLayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(layout, |_| Ok(()), cx)
        });
        poll(&view, &program, cx);
        assert!(cx.debug_bounds("chat-waiting").is_some(), "{layout:?}");
        assert!(cx.debug_bounds("chat-typing").is_none(), "{layout:?}");
    }
}

#[gpui::test]
fn the_daemon_says_when_a_mirrored_agent_waits(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut message = snapshot(cursor(1, 0), vec![agent(1, "Mirrored")]);
    if let ServerMessage::Snapshot { transcript, .. } = &mut message {
        transcript.read_only = true;
        transcript.status = Status::Starting;
    }
    let _stream = attach_with(&view, &program, message, cx);
    for (reported, waits) in [(AgentStatus::Working, false), (AgentStatus::Blocked, true)] {
        view.update(cx, |view, _| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status = reported;
        });
        poll(&view, &program, cx);
        assert_eq!(cx.debug_bounds("chat-waiting").is_some(), waits);
    }
}

#[gpui::test]
fn a_chat_with_no_composer_shows_its_context_in_the_header(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut message = snapshot(cursor(1, 0), vec![agent(1, "Mirrored")]);
    if let ServerMessage::Snapshot { transcript, .. } = &mut message {
        transcript.read_only = true;
        transcript.usage = Some(Usage {
            cost: None,
            size: 200_000,
            used: 50_000,
        });
    }
    let _stream = attach_with(&view, &program, message, cx);
    poll(&view, &program, cx);
    let usage = cx.debug_bounds("chat-usage").unwrap();
    let terminal = cx.debug_bounds("chat-terminal").unwrap();
    assert!(
        usage.right() <= terminal.left(),
        "beside the header's buttons"
    );
    assert!(usage.bottom() <= terminal.bottom() + gpui::px(8.));
}
