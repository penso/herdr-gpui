//! An empty chat lists its modes, so a carried-over mode is seen and can be
//! changed before the first prompt.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::chat::view::mode_rows;
use gpui::{Modifiers, TestAppContext};
use sheprd_chat_protocol::{
    Choice, Command, ConfigOption, Mode, Modes, OptionCategory, OptionKind, OptionValue,
};
use sheprd_client::ConnectTarget;

fn next_command(stream: &mut Peer) -> Command {
    match read(stream) {
        Some(ClientMessage::Command { command, .. }) => command,
        other => panic!("a command, got {other:?}"),
    }
}

fn with_modes(entries: Vec<Entry>) -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = entries;
    transcript.modes = Some(Modes {
        available: ["default", "bypassPermissions"]
            .into_iter()
            .map(|id| Mode {
                description: None,
                id: id.into(),
                name: id.into(),
            })
            .collect(),
        current: "bypassPermissions".into(),
    });
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

#[test]
fn the_mode_setting_wins_and_marks_the_current_choice() {
    let mut transcript = Transcript::new("OpenCode");
    assert!(mode_rows(&transcript).is_empty());
    transcript.modes = Some(Modes {
        available: vec![Mode {
            description: None,
            id: "old".into(),
            name: "Old".into(),
        }],
        current: "old".into(),
    });
    let choice = |id: &str| Choice {
        description: None,
        group: None,
        id: id.into(),
        name: id.into(),
    };
    transcript.options = vec![ConfigOption {
        category: OptionCategory::Mode,
        description: None,
        id: "mode".into(),
        kind: OptionKind::Select {
            choices: vec![choice("build"), choice("bypass")],
            current: "bypass".into(),
        },
        name: "Mode".into(),
    }];
    let rows = mode_rows(&transcript);
    assert_eq!(rows.len(), 2);
    assert!(!rows[0].current && !rows[0].bypass);
    assert!(rows[1].current && rows[1].bypass);
    assert_eq!(
        rows[0].command,
        Command::SetOption {
            id: "mode".into(),
            value: OptionValue::Choice { id: "build".into() },
        }
    );
}

#[gpui::test]
fn an_empty_chat_changes_its_mode_from_the_panel(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, with_modes(Vec::new()), cx);
    assert!(cx.debug_bounds("chat-opening").is_some());
    let row = cx.debug_bounds("chat-opening-mode-0").unwrap();
    cx.simulate_click(row.center(), Modifiers::default());
    assert_eq!(
        next_command(&mut stream),
        Command::SetMode {
            id: "default".into()
        }
    );
}

#[gpui::test]
fn the_panel_leaves_with_the_first_message(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, with_modes(Vec::new()), cx);
    assert!(cx.debug_bounds("chat-opening").is_some());
    send(
        &mut stream,
        &change(
            Change::Push {
                entry: agent(1, "Hi."),
            },
            cursor(1, 1),
        ),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        cx.debug_bounds("chat-opening").is_none()
    });
}

#[gpui::test]
fn a_chat_with_messages_shows_no_panel(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, with_modes(vec![agent(1, "Hi.")]), cx);
    assert!(cx.debug_bounds("chat-opening").is_none());
}
