use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::config::ChatLayoutMode;
use gpui::TestAppContext;
use herdr_chat_protocol::{Turn, Usage};
use herdr_client::ConnectTarget;

#[gpui::test]
fn every_layout_keeps_live_activity_above_input_and_stops_animation_when_idle(
    cx: &mut TestAppContext,
) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = Transcript::new("Custom ACP");
    transcript.status = Status::Working;
    transcript.turn = Some(Turn {
        started_ms: now_ms() - 10_000,
    });
    transcript.usage = Some(Usage {
        cost: None,
        used: 1234,
        size: 64000,
    });
    transcript.entries = vec![Entry {
        id: EntryId(1),
        kind: EntryKind::Thought {
            text: "Reported reasoning".into(),
        },
    }];
    let mut stream = attach_with(
        &view,
        &program,
        ServerMessage::Snapshot {
            cursor: cursor(1, 0),
            transcript: Box::new(transcript),
        },
        cx,
    );
    for &layout in ChatLayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(layout, |_| Ok(()), cx)
        });
        poll(&view, &program, cx);
        let activity = cx.debug_bounds("chat-live-activity").unwrap();
        let composer = cx.debug_bounds("chat-composer").unwrap();
        assert!(activity.bottom() <= composer.top(), "{layout:?}");
        let moving = if layout == ChatLayoutMode::IMessage {
            "chat-typing"
        } else {
            "chat-live-moving"
        };
        assert!(cx.debug_bounds(moving).is_some());
        assert!(cx.debug_bounds("chat-live-metrics").is_some());
    }
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
        cx.debug_bounds("chat-live-moving").is_none() && cx.debug_bounds("chat-typing").is_none()
    });
    assert!(cx.debug_bounds("chat-live-activity").is_some());
}

#[gpui::test]
fn an_observed_blocked_agent_shows_a_static_waiting_strip(cx: &mut TestAppContext) {
    let (view, _peer, _program, _stream, cx) = terminal_input::observed(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let pane = view.chats.pane_mut("w1:p1").unwrap();
            pane.session.set_observed_status(Status::Working);
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status =
                herdr_client::protocol::AgentStatus::Blocked;
            view.redraw_terminal(cx);
        });
        crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("chat-live-activity").is_some());
    assert!(cx.debug_bounds("chat-live-moving").is_none());
    assert!(view.read_with(cx, |view, _| {
        view.chat_needs_input(view.chats.pane("w1:p1").unwrap())
    }));
}
