use super::*;
use crate::assist::{History, completions, next_mode, step};

fn command(name: &str) -> SlashCommand {
    SlashCommand {
        description: String::new(),
        hint: None,
        name: name.to_owned(),
    }
}

fn select(id: &str, category: OptionCategory, choices: &[&str], current: &str) -> ConfigOption {
    ConfigOption {
        category,
        description: None,
        id: id.to_owned(),
        kind: OptionKind::Select {
            choices: choices
                .iter()
                .map(|choice| Choice {
                    description: None,
                    group: None,
                    id: (*choice).to_owned(),
                    name: choice.to_uppercase(),
                })
                .collect(),
            current: current.to_owned(),
        },
        name: id.to_owned(),
    }
}

#[test]
fn slash_commands_complete_a_name_being_typed() {
    let mut transcript = Transcript::new("Claude");
    transcript.commands = vec![command("review"), command("reset"), command("init")];
    let names = |text| {
        completions(&transcript, text)
            .into_iter()
            .map(|command| command.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names("/re"), ["review", "reset"]);
    assert_eq!(names("/"), ["review", "reset", "init"]);
    assert!(names("/review").is_empty(), "already complete");
    assert!(names("/review this").is_empty());
    assert!(names("re").is_empty());
}

#[test]
fn history_steps_back_and_forth() {
    let mut transcript = Transcript::new("Claude");
    let mut history = History::default();
    assert_eq!(history.older(&transcript), None);
    transcript.entries = vec![
        user(1, "one"),
        agent(2, "ok"),
        user(3, "two"),
        user(4, "two"),
    ];
    assert_eq!(history.older(&transcript), Some("two"));
    assert_eq!(history.older(&transcript), Some("one"));
    assert_eq!(history.older(&transcript), Some("one"));
    assert_eq!(history.newer(), Some("two"));
    assert_eq!(history.newer(), Some(""));
    assert!(!history.recalling());
}

fn with_files(id: u64, text: &str, files: usize) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            text: text.to_owned(),
            files,
        },
    }
}

#[test]
fn history_recalls_prompts_from_every_client_and_the_queue() {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![
        user(1, "replayed"),
        with_files(2, "look\n\u{1f4ce} notes.md\n\u{1f4ce} shot.png", 2),
        with_files(3, "\u{1f4ce} only.png", 1),
    ];
    transcript.queued = vec![Queued {
        attachments: Vec::new(),
        id: CommandId::random(),
        text: "waiting".to_owned(),
    }];
    let mut history = History::default();
    assert_eq!(history.older(&transcript), Some("waiting"));
    assert_eq!(
        history.older(&transcript),
        Some("look"),
        "files are not text"
    );
    assert_eq!(history.older(&transcript), Some("replayed"));
}

#[test]
fn history_keeps_typed_lines_that_look_like_files() {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![
        user(1, "\u{1f4ce} typed"),
        user(2, "summarize\n\u{1f4ce} keep this line"),
        with_files(3, "see\n\u{1f4ce} mine\n\u{1f4ce} shot.png", 1),
    ];
    let mut history = History::default();
    assert_eq!(history.older(&transcript), Some("see\n\u{1f4ce} mine"));
    assert_eq!(
        history.older(&transcript),
        Some("summarize\n\u{1f4ce} keep this line")
    );
    assert_eq!(history.older(&transcript), Some("\u{1f4ce} typed"));
}

#[test]
fn history_recalls_a_sent_prompt_before_the_program_shows_it() {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![user(1, "one")];
    let mut history = History::default();
    history.push("two", &transcript);
    assert_eq!(history.older(&transcript), Some("two"));
    assert_eq!(history.older(&transcript), Some("one"));

    // Once shown, it is recalled once, and later prompts come after it.
    history.stop();
    transcript
        .entries
        .extend([user(2, "two"), user(3, "three")]);
    assert_eq!(history.older(&transcript), Some("three"));
    assert_eq!(history.older(&transcript), Some("two"));
    assert_eq!(history.older(&transcript), Some("one"));
}

#[test]
fn history_recalls_a_repeated_prompt_first_before_its_echo() {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![user(1, "one"), user(2, "two")];
    let mut history = History::default();
    history.push("one", &transcript);
    assert_eq!(history.older(&transcript), Some("one"));
    assert_eq!(history.older(&transcript), Some("two"));

    history.stop();
    transcript.entries.push(user(3, "one"));
    assert_eq!(history.older(&transcript), Some("one"));
    assert_eq!(history.older(&transcript), Some("two"), "recalled once");
}

#[test]
fn history_knows_a_sent_prompts_echo_after_the_transcript_trims() {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![user(1, "one"), user(2, "two")];
    let mut history = History::default();
    history.push("three", &transcript);

    // The oldest prompt is trimmed away before the echo arrives, and
    // another client sends a prompt after it.
    transcript.entries = vec![user(2, "two"), user(3, "three"), user(4, "four")];
    assert_eq!(history.older(&transcript), Some("four"));
    assert_eq!(history.older(&transcript), Some("three"));
    assert_eq!(history.older(&transcript), Some("two"), "recalled once");
}

#[test]
fn history_knows_a_sent_prompt_that_waits_in_the_queue() {
    let waiting = |text: &str| Queued {
        attachments: Vec::new(),
        id: CommandId::random(),
        text: text.to_owned(),
    };
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![user(1, "one")];
    transcript.queued = vec![waiting("two")];
    let mut history = History::default();
    history.push("two", &transcript);
    transcript.queued.push(waiting("two"));
    transcript.queued.push(waiting("three"));
    assert_eq!(history.older(&transcript), Some("three"));
    assert_eq!(history.older(&transcript), Some("two"));
    assert_eq!(history.older(&transcript), Some("one"), "recalled once");
}

#[test]
fn history_steps_through_what_it_had_when_recall_started() {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![user(1, "one"), user(2, "two")];
    let mut history = History::default();
    assert_eq!(history.older(&transcript), Some("two"));
    transcript.entries.push(user(3, "three"));
    assert_eq!(history.older(&transcript), Some("one"));
    assert_eq!(history.newer(), Some("two"));
}

#[test]
fn settings_step_through_choices_and_wrap() {
    let model = select("model", OptionCategory::Model, &["a", "b", "c"], "c");
    assert_eq!(
        step(&model, 1),
        Some(Command::SetOption {
            id: "model".into(),
            value: OptionValue::Choice { id: "a".into() }
        })
    );
    assert_eq!(
        step(&model, -1),
        Some(Command::SetOption {
            id: "model".into(),
            value: OptionValue::Choice { id: "b".into() }
        })
    );
    let alone = select("model", OptionCategory::Model, &["a"], "a");
    assert_eq!(step(&alone, 1), None);
    let toggle = ConfigOption {
        category: OptionCategory::Other,
        description: None,
        id: "fast".into(),
        kind: OptionKind::Toggle { current: true },
        name: "Fast".into(),
    };
    assert_eq!(
        step(&toggle, 1),
        Some(Command::SetOption {
            id: "fast".into(),
            value: OptionValue::Toggle { value: false }
        })
    );
}

#[test]
fn the_mode_setting_wins_over_the_older_modes() {
    let mut transcript = Transcript::new("Claude");
    transcript.modes = Some(Modes {
        available: vec![
            Mode {
                description: None,
                id: "default".into(),
                name: "Default".into(),
            },
            Mode {
                description: None,
                id: "plan".into(),
                name: "Plan".into(),
            },
        ],
        current: "default".into(),
    });
    assert_eq!(
        next_mode(&transcript),
        Some(Command::SetMode { id: "plan".into() })
    );
    transcript.options = vec![select(
        "mode",
        OptionCategory::Mode,
        &["ask", "auto"],
        "ask",
    )];
    assert_eq!(
        next_mode(&transcript),
        Some(Command::SetOption {
            id: "mode".into(),
            value: OptionValue::Choice { id: "auto".into() }
        })
    );
}
