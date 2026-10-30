//! What the agent left running is said on the working row in every layout,
//! even after the turn ends, opens into a table, and clears once it is done.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::config::ChatLayoutMode;
use gpui::{Modifiers, TestAppContext, px};
use herdr_chat_protocol::{Activity, ActivityKind, ActivityState, Change};
use herdr_client::ConnectTarget;

fn shell(state: ActivityState) -> Activity {
    Activity {
        call_id: "toolu_bg".into(),
        detail: None,
        finished_ms: (state != ActivityState::Running).then_some(now_ms()),
        kind: ActivityKind::Shell,
        label: "npm run dev".into(),
        started_ms: now_ms().saturating_sub(65_000),
        state,
    }
}

/// An idle chat whose background shell still runs.
fn idle_with_a_shell() -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![agent(1, "The dev server runs in the background.")];
    transcript.activities = vec![shell(ActivityState::Running)];
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

#[gpui::test]
fn a_shell_still_running_after_the_turn_shows_in_every_layout(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, idle_with_a_shell(), cx);
    for &layout in ChatLayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(layout, |_| Ok(()), cx)
        });
        poll(&view, &program, cx);
        let running = cx
            .debug_bounds("chat-running")
            .unwrap_or_else(|| panic!("{layout:?}"));
        let composer = cx.debug_bounds("chat-composer").unwrap();
        assert!(
            running.bottom() <= composer.top(),
            "{layout:?}: above the composer"
        );
        assert!(cx.debug_bounds("chat-working").is_none(), "{layout:?}");
    }
}

#[gpui::test]
fn the_row_opens_a_table_and_clears_once_everything_ended(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, idle_with_a_shell(), cx);
    assert!(cx.debug_bounds("chat-running-table").is_none());
    let row = cx.debug_bounds("chat-running").unwrap();
    cx.simulate_click(row.center(), Modifiers::default());
    // Once it has grown all the way in.
    let opened = Instant::now();
    wait_until(|| {
        poll(&view, &program, cx);
        opened.elapsed() > crate::motion::ENTER * 2
            && cx.debug_bounds("chat-running-row-0").is_some()
    });
    // The table pushes the row up rather than covering the conversation.
    let row = cx.debug_bounds("chat-running").unwrap();
    let table = cx.debug_bounds("chat-running-table").unwrap();
    let composer = cx.debug_bounds("chat-composer").unwrap();
    assert!(table.top() >= row.bottom() - px(1.), "under the row");
    assert!(
        table.bottom() <= composer.top() + px(1.),
        "above the composer"
    );

    send(
        &mut stream,
        &ServerMessage::Change {
            change: Change::Activity {
                activity: shell(ActivityState::Finished),
            },
            cursor: cursor(1, 1),
        },
    );
    wait_until(|| {
        poll(&view, &program, cx);
        cx.debug_bounds("chat-running").is_none()
    });
    assert!(cx.debug_bounds("chat-running-table").is_none());
}
