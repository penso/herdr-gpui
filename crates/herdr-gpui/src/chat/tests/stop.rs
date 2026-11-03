//! The composer's round button, as in the phone app: it stops a running
//! turn while nothing is written, sends once something is, and does
//! nothing otherwise.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::chat::chats::SendButton;
use gpui::{Modifiers, TestAppContext};
use herdr_chat_protocol::Command;
use herdr_client::ConnectTarget;

fn working() -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.entries = vec![agent(1, "Thinking.")];
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

fn button(view: &gpui::Entity<crate::HerdrWindow>, cx: &mut gpui::VisualTestContext) -> SendButton {
    view.read_with(cx, |view, _| view.chats.panes().next().unwrap().send)
}

fn next_command(stream: &mut Peer) -> Command {
    match read(stream) {
        Some(ClientMessage::Command { command, .. }) => command,
        other => panic!("a command, got {other:?}"),
    }
}

#[gpui::test]
fn the_send_button_stops_a_running_turn(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, working(), cx);
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-send").is_none());
    let stop = cx.debug_bounds("chat-stop").unwrap();
    cx.simulate_click(stop.center(), Modifiers::default());
    assert_eq!(next_command(&mut stream), Command::Cancel);
}

#[gpui::test]
fn a_written_prompt_turns_stop_back_into_send(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, working(), cx);
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-stop").is_some());

    cx.simulate_input("next");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-stop").is_none());
    assert!(cx.debug_bounds("chat-send").is_some());

    // Taking the text back brings the stop button back.
    cx.simulate_keystrokes("backspace backspace backspace backspace");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-stop").is_some());
}

#[gpui::test]
fn send_stays_off_until_something_is_written(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut idle = Transcript::new("Claude");
    idle.status = Status::Idle;
    let snapshot = ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(idle),
    };
    let mut stream = attach_with(&view, &program, snapshot, cx);
    poll(&view, &program, cx);
    assert_eq!(button(&view, cx), SendButton::Off);
    assert!(cx.debug_bounds("chat-stop").is_none());
    // Blank text is nothing to send either.
    cx.simulate_input("  ");
    poll(&view, &program, cx);
    assert_eq!(button(&view, cx), SendButton::Off);
    let send = cx.debug_bounds("chat-send").unwrap();
    cx.simulate_click(send.center(), Modifiers::default());

    cx.simulate_input("hi");
    poll(&view, &program, cx);
    assert_eq!(button(&view, cx), SendButton::Send);
    cx.simulate_click(send.center(), Modifiers::default());
    // The first command the program sees is the written prompt, so the
    // click while off sent nothing.
    match next_command(&mut stream) {
        Command::Prompt { text, .. } => assert_eq!(text.trim(), "hi"),
        other => panic!("the prompt, got {other:?}"),
    }
}
