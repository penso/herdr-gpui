use super::*;
use herdr_chat_protocol::{
    Choice, ConfigOption, Mode, Modes, OptionCategory, OptionKind, SlashCommand,
};

fn choice(id: &str, description: Option<&str>) -> Choice {
    Choice {
        description: description.map(Into::into),
        group: None,
        id: id.into(),
        name: id.into(),
    }
}

fn select(id: &str, category: OptionCategory, choices: Vec<Choice>, current: &str) -> ConfigOption {
    ConfigOption {
        category,
        description: None,
        id: id.into(),
        kind: OptionKind::Select {
            choices,
            current: current.into(),
        },
        name: id.into(),
    }
}

fn configured() -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.options = vec![
        select(
            "model",
            OptionCategory::Model,
            vec![
                choice("Opus 5.5", Some("Most capable")),
                choice("Sonnet 5.5", None),
            ],
            "Opus 5.5",
        ),
        select(
            "effort",
            OptionCategory::ThoughtLevel,
            vec![
                choice("Low", None),
                choice("Medium", None),
                choice("High", None),
            ],
            "High",
        ),
        ConfigOption {
            category: OptionCategory::Other,
            description: None,
            id: "fast".into(),
            kind: OptionKind::Toggle { current: true },
            name: "Fast mode".into(),
        },
    ];
    transcript.modes = Some(Modes {
        available: vec![
            Mode {
                description: Some("Runs every tool without asking".into()),
                id: "bypass".into(),
                name: "Bypass Permissions".into(),
            },
            Mode {
                description: None,
                id: "ask".into(),
                name: "Ask".into(),
            },
        ],
        current: "bypass".into(),
    });
    transcript
}

fn view(transcript: Transcript) -> View {
    View {
        transcript,
        connected: true,
    }
}

#[test]
fn the_title_opens_the_settings_and_sums_them_up() {
    let html = chat(&workspace(vec![]), &row("p1"), &view(configured()), 0).into_string();
    let head = &html[..html.find(r#"id="chat-turn""#).unwrap()];
    assert!(head.contains("data-open-settings"), "{head}");
    assert!(
        head.contains("Opus 5.5 · High · Bypass Permissions"),
        "{head}"
    );
    assert!(!head.contains("<select"), "{head}");
}

#[test]
fn a_chat_without_settings_has_a_plain_title() {
    let html = chat(
        &workspace(vec![]),
        &row("p1"),
        &view(Transcript::new("Claude")),
        0,
    )
    .into_string();
    assert!(!html.contains("data-open-settings"));
    assert!(!html.contains("settings-sheet"));
}

#[test]
fn short_plain_choices_are_segments_and_described_ones_a_list() {
    let html = session::sheet(&workspace(vec![]), &row("p1"), &configured()).into_string();
    let effort = &html[html.find(">effort<").unwrap()..];
    let effort = &effort[..effort.find("</div>").unwrap()];
    assert!(effort.contains(r#"class="segmented""#), "{effort}");
    assert!(!effort.contains("settings-choice"), "{effort}");
    assert!(
        html.contains(r#"value="High" data-option="effort" data-pane="p1" checked"#),
        "{html}"
    );
    let model = &html[html.find(">model<").unwrap()..html.find(">effort<").unwrap()];
    assert!(
        model.contains("settings-choice") && model.contains("Most capable"),
        "{model}"
    );
    assert!(
        html.contains(r#"data-toggle="fast" data-pane="p1" checked"#),
        "{html}"
    );
    assert!(
        html.contains(r#"value="bypass" data-mode data-pane="p1" checked"#),
        "{html}"
    );
    assert!(html.contains("Runs every tool without asking"));
    assert!(html.contains("data-auto-accept"));
}

#[test]
fn the_composer_lists_commands_for_a_typed_slash() {
    let mut transcript = Transcript::new("Claude");
    transcript.commands = vec![SlashCommand {
        description: "Publish a release".into(),
        hint: None,
        name: "release".into(),
    }];
    let html = chat(&workspace(vec![]), &row("p1"), &view(transcript), 0).into_string();
    let composer = &html[html.find(r#"id="chat-composer""#).unwrap()..];
    assert!(
        composer.contains(r#"class="slash-list" hidden"#),
        "{composer}"
    );
    assert!(composer.contains(r#"data-slash="release""#));
    assert!(composer.contains("Publish a release"));
    assert!(
        !composer.contains("＋") && !composer.contains("↑"),
        "{composer}"
    );
}
