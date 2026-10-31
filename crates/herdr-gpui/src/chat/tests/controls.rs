//! The chat header's settings, usage meter, and turn timer; slash-command
//! completion and prompt recall in the composer; turn summaries and copying
//! in the entries.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::{Modifiers, TestAppContext};
use herdr_chat_protocol::{
    Choice, Command, ConfigOption, Cost, OptionCategory, OptionKind, OptionValue, SlashCommand,
    Turn, TurnEnd, TurnSummary, Usage,
};
use herdr_client::ConnectTarget;

fn select(id: &str, category: OptionCategory, choices: &[&str], current: &str) -> ConfigOption {
    ConfigOption {
        category,
        description: None,
        id: id.into(),
        kind: OptionKind::Select {
            choices: choices
                .iter()
                .map(|choice| Choice {
                    description: None,
                    group: Some("Group".into()),
                    id: (*choice).into(),
                    name: choice.to_uppercase(),
                })
                .collect(),
            current: current.into(),
        },
        name: id.into(),
    }
}

/// A transcript with settings, usage, commands, and a finished turn.
fn rich() -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![
        agent(1, "Hello."),
        Entry {
            id: EntryId(2),
            kind: EntryKind::Turn(TurnSummary {
                ended_ms: None,
                duration_ms: 12_000,
                end: TurnEnd::Done,
                tokens: None,
            }),
        },
    ];
    transcript.options = vec![
        select("model", OptionCategory::Model, &["opus", "sonnet"], "opus"),
        select(
            "effort",
            OptionCategory::ThoughtLevel,
            &["low", "high"],
            "high",
        ),
        ConfigOption {
            category: OptionCategory::Other,
            description: None,
            id: "fast".into(),
            kind: OptionKind::Toggle { current: false },
            name: "Fast".into(),
        },
    ];
    transcript.usage = Some(Usage {
        cost: Some(Cost {
            currency: "USD".into(),
            micros: 420_000,
        }),
        size: 200_000,
        used: 50_000,
    });
    transcript.commands = vec![SlashCommand {
        description: "Review the changes".into(),
        hint: None,
        name: "review".into(),
    }];
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

fn next_command(stream: &mut Peer) -> Command {
    match read(stream) {
        Some(ClientMessage::Command { command, .. }) => command,
        other => panic!("a command, got {other:?}"),
    }
}

#[gpui::test]
fn the_header_shows_settings_usage_and_turns(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, rich(), cx);
    for selector in [
        "chat-setting-model",
        "chat-setting-effort",
        "chat-more-settings",
        "chat-usage",
        "chat-turn",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    // A mode setting would replace the older mode button; none here.
    assert!(cx.debug_bounds("chat-mode").is_none());
}

#[gpui::test]
fn picking_a_model_sends_the_setting(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, rich(), cx);
    let button = cx.debug_bounds("chat-setting-model").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-menu").is_some());
    let sonnet = cx.debug_bounds("chat-menu-row-1").unwrap();
    cx.simulate_click(sonnet.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-menu").is_none());
    assert_eq!(
        next_command(&mut stream),
        Command::SetOption {
            id: "model".into(),
            value: OptionValue::Choice {
                id: "sonnet".into()
            }
        }
    );
}

#[gpui::test]
fn the_more_menu_toggles_a_setting_without_a_button(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, rich(), cx);
    let more = cx.debug_bounds("chat-more-settings").unwrap();
    cx.simulate_click(more.center(), Modifiers::default());
    poll(&view, &program, cx);
    let fast = cx.debug_bounds("chat-menu-row-0").unwrap();
    cx.simulate_click(fast.center(), Modifiers::default());
    assert_eq!(
        next_command(&mut stream),
        Command::SetOption {
            id: "fast".into(),
            value: OptionValue::Toggle { value: true }
        }
    );
}

#[gpui::test]
fn a_slash_completes_the_agents_commands(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, rich(), cx);
    cx.simulate_input("/re");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-completions").is_some());
    cx.simulate_keystrokes("tab");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-completions").is_none());
    cx.simulate_input("src");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        next_command(&mut stream),
        Command::Prompt {
            text: "/review src".into(),
            attachments: Vec::new(),
        }
    );
}

#[gpui::test]
fn up_recalls_the_last_prompt(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, rich(), cx);
    cx.simulate_input("first try");
    cx.simulate_keystrokes("enter");
    next_command(&mut stream);
    cx.simulate_keystrokes("up");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        next_command(&mut stream),
        Command::Prompt {
            text: "first try".into(),
            attachments: Vec::new(),
        }
    );
}

#[gpui::test]
fn a_running_turn_shows_how_long_it_has_taken(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, rich(), cx);
    let started = now_ms() - 5_000;
    send(
        &mut stream,
        &change(
            Change::Turn {
                turn: Some(Turn {
                    started_ms: started,
                }),
            },
            cursor(1, 1),
        ),
    );
    send(
        &mut stream,
        &change(
            Change::Status {
                status: Status::Working,
            },
            cursor(1, 2),
        ),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            view.chats
                .pane("w1:p1")
                .is_some_and(|pane| pane.session.transcript().status == Status::Working)
        })
    });
    let text = view.read_with(cx, |view, _| {
        view::status_text(view.chats.pane("w1:p1").unwrap().session.transcript())
    });
    assert!(
        text.starts_with("Working 5s") || text.starts_with("Working 6s"),
        "{text}"
    );
}
