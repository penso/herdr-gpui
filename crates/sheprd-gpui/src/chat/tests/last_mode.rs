//! A chat this app starts takes the mode last picked for its agent, once its
//! session is up; any other chat keeps its own.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::{
    SheprdWindow,
    chat::{LastModes, last_mode::Fresh},
};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use sheprd_chat_protocol::{Command, Mode, Modes};
use sheprd_client::ConnectTarget;

fn next_command(stream: &mut Peer) -> Command {
    match read(stream) {
        Some(ClientMessage::Command { command, .. }) => command,
        other => panic!("a command, got {other:?}"),
    }
}

fn transcript(status: Status) -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.status = status;
    transcript.modes = Some(modes());
    transcript
}

fn modes() -> Modes {
    Modes {
        available: ["default", "plan"]
            .into_iter()
            .map(|id| Mode {
                description: None,
                id: id.into(),
                name: id.into(),
            })
            .collect(),
        current: "default".into(),
    }
}

fn greeting(transcript: Transcript) -> ServerMessage {
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

fn remember_plan(cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        LastModes::update(cx, |last| {
            last.remember("Claude", Command::SetMode { id: "plan".into() });
            last.remember("Claude", Command::SetAutoAccept { on: true });
        });
    });
}

/// Marks the fixture's pane as a chat this app just started.
fn expect_fresh(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| {
        let pane = view.live.snapshot.as_ref().unwrap().agents[0]
            .pane_id
            .clone();
        view.chats.expect_fresh(Fresh::Pane(pane), Instant::now());
    });
}

/// Sends a command that proves what went out before it.
fn cancel(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        let pane = view.chats.panes().next().unwrap().target.pane_id.clone();
        view.chat_send(&pane, Command::Cancel, cx);
    });
}

#[gpui::test]
fn a_started_chat_takes_the_choices_once(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    remember_plan(cx);
    expect_fresh(&view, cx);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, greeting(transcript(Status::Idle)), cx);
    assert_eq!(
        next_command(&mut stream),
        Command::SetMode { id: "plan".into() }
    );
    assert_eq!(
        next_command(&mut stream),
        Command::SetAutoAccept { on: true }
    );
    poll(&view, &program, cx);
    cancel(&view, cx);
    assert_eq!(next_command(&mut stream), Command::Cancel);
}

#[gpui::test]
fn a_starting_session_waits_until_it_is_up(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    remember_plan(cx);
    expect_fresh(&view, cx);
    let program = Program::new();
    let mut starting = Transcript::new("Claude");
    starting.status = Status::Starting;
    let mut stream = attach_with(&view, &program, greeting(starting), cx);
    poll(&view, &program, cx);
    send(
        &mut stream,
        &change(
            Change::Modes {
                modes: Some(modes()),
            },
            cursor(1, 1),
        ),
    );
    send(
        &mut stream,
        &change(
            Change::Status {
                status: Status::Idle,
            },
            cursor(1, 2),
        ),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| view.chats.fresh.is_empty())
    });
    assert_eq!(
        next_command(&mut stream),
        Command::SetMode { id: "plan".into() }
    );
}

#[gpui::test]
fn any_other_chat_keeps_its_mode(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    remember_plan(cx);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, greeting(transcript(Status::Idle)), cx);
    poll(&view, &program, cx);
    cancel(&view, cx);
    assert_eq!(next_command(&mut stream), Command::Cancel);
}

#[gpui::test]
fn picking_a_mode_in_a_chat_is_remembered(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, greeting(transcript(Status::Idle)), cx);
    let mode = cx.debug_bounds("chat-mode").unwrap();
    cx.simulate_click(mode.center(), Modifiers::default());
    assert_eq!(
        next_command(&mut stream),
        Command::SetMode { id: "plan".into() }
    );
    // The next chat with this agent starts there, here shown on the same pane.
    expect_fresh(&view, cx);
    poll(&view, &program, cx);
    assert_eq!(
        next_command(&mut stream),
        Command::SetMode { id: "plan".into() }
    );
}
