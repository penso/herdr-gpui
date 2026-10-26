//! A read-only pane that is not a discovered agent, such as a
//! `[chat.mirrors]` entry, takes its status from the daemon too.

use super::overlay::{attach_with, open, poll};
use super::*;
use gpui::TestAppContext;
use herdr_client::{ConnectTarget, protocol::AgentStatus};
use std::sync::Arc;

#[gpui::test]
fn a_mirrored_pane_follows_the_daemons_agent_status(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = snapshot(cursor(1, 0), vec![agent(1, "Mirrored")]);
    if let ServerMessage::Snapshot { transcript, .. } = &mut transcript {
        transcript.read_only = true;
        transcript.status = Status::Starting;
    }
    let _stream = attach_with(&view, &program, transcript, cx);
    let status = |view: &gpui::Entity<crate::HerdrWindow>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            let pane = view.chats.pane("w1:p1").unwrap();
            assert!(pane.target.existing.is_none());
            pane.session.transcript().status.clone()
        })
    };
    for (reported, expected) in [
        (AgentStatus::Working, Status::Working),
        (AgentStatus::Idle, Status::Idle),
    ] {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.agents[0].agent_status = reported;
        });
        poll(&view, &program, cx);
        assert_eq!(status(&view, cx), expected);
    }
}
