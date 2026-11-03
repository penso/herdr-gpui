//! The composer's settings: a two-way setting is a switch, and a long menu
//! such as the model list opens with a search field.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::chat::view::matching;
use gpui::{Modifiers, TestAppContext};
use sheprd_chat_protocol::{
    Choice, Command, ConfigOption, Mode, Modes, OptionCategory, OptionKind, OptionValue,
};
use sheprd_client::ConnectTarget;

fn choice(id: &str) -> Choice {
    Choice {
        description: None,
        group: Some("OpenAI".into()),
        id: id.into(),
        name: format!("OpenAI/{id}"),
    }
}

fn select(id: &str, category: OptionCategory, choices: &[&str]) -> ConfigOption {
    ConfigOption {
        category,
        description: None,
        id: id.into(),
        kind: OptionKind::Select {
            choices: choices.iter().map(|id| choice(id)).collect(),
            current: choices[0].into(),
        },
        name: id.into(),
    }
}

const MODELS: [&str; 10] = [
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.5",
    "gpt-5.5-fast",
    "gpt-5.6-luna",
    "gpt-5.6-luna-fast",
    "gpt-5.6-sol",
    "gpt-5.6-sol-fast",
    "codex-spark",
    "big-pickle",
];

fn settings() -> ServerMessage {
    let mut transcript = Transcript::new("OpenCode");
    transcript.status = Status::Idle;
    transcript.entries = vec![agent(1, "Hi.")];
    transcript.options = vec![
        select("model", OptionCategory::Model, &MODELS),
        select("mode", OptionCategory::Mode, &["build", "plan"]),
    ];
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

#[test]
fn search_matches_names_and_groups_ignoring_case() {
    let choices: Vec<_> = MODELS.iter().map(|id| choice(id)).collect();
    let ids = |query: &str| -> Vec<&str> {
        matching(&choices, query)
            .map(|(_, choice)| choice.id.as_str())
            .collect()
    };
    assert_eq!(ids("SOL"), ["gpt-5.6-sol", "gpt-5.6-sol-fast"]);
    assert_eq!(ids("openai").len(), MODELS.len());
    assert_eq!(ids("  ").len(), MODELS.len());
    assert!(ids("claude").is_empty());
}

#[gpui::test]
fn a_two_way_setting_is_a_switch(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, settings(), cx);
    let plan = cx.debug_bounds("chat-setting-mode").unwrap();
    cx.simulate_click(plan.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-menu").is_none(), "no menu opens");
    assert_eq!(
        next_command(&mut stream),
        Command::SetOption {
            id: "mode".into(),
            value: OptionValue::Choice { id: "plan".into() },
        }
    );
}

#[gpui::test]
fn older_two_way_modes_are_a_switch(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.modes = Some(Modes {
        available: ["default", "plan"]
            .into_iter()
            .map(|id| Mode {
                description: None,
                id: id.into(),
                name: id.into(),
            })
            .collect(),
        current: "default".into(),
    });
    let snapshot = ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    };
    let mut stream = attach_with(&view, &program, snapshot, cx);
    let plan = cx.debug_bounds("chat-mode").unwrap();
    cx.simulate_click(plan.center(), Modifiers::default());
    assert_eq!(
        next_command(&mut stream),
        Command::SetMode { id: "plan".into() }
    );
}

#[gpui::test]
fn the_model_menu_searches_and_enter_picks_the_first_match(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, settings(), cx);
    let model = cx.debug_bounds("chat-setting-model").unwrap();
    cx.simulate_click(model.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-menu-search").is_some());
    assert!(cx.debug_bounds("chat-menu-row-0").is_some());
    cx.simulate_input("sol");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-menu-row-0").is_none(), "filtered out");
    assert!(cx.debug_bounds("chat-menu-row-6").is_some());
    cx.simulate_keystrokes("enter");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-menu").is_none());
    assert_eq!(
        next_command(&mut stream),
        Command::SetOption {
            id: "model".into(),
            value: OptionValue::Choice {
                id: "gpt-5.6-sol".into()
            },
        }
    );
    // The composer has the keyboard again, and the search text never
    // reached it.
    cx.simulate_input("hello");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        next_command(&mut stream),
        Command::Prompt {
            text: "hello".into(),
            attachments: Vec::new(),
        }
    );
}

#[gpui::test]
fn escape_closes_the_menu_search(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, settings(), cx);
    let model = cx.debug_bounds("chat-setting-model").unwrap();
    cx.simulate_click(model.center(), Modifiers::default());
    poll(&view, &program, cx);
    cx.simulate_keystrokes("escape");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-menu").is_none());
}

#[gpui::test]
fn the_shield_turns_auto_accepting_on(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, settings(), cx);
    let shield = cx.debug_bounds("chat-auto-accept").unwrap();
    cx.simulate_click(shield.center(), Modifiers::default());
    assert_eq!(
        next_command(&mut stream),
        Command::SetAutoAccept { on: true }
    );
}
