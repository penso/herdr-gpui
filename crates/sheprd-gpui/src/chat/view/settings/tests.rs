#![allow(clippy::unwrap_used)]
use super::*;
use core::prelude::v1::test;
use gpui::AssetSource;
use sheprd_chat_protocol::{Choice, ConfigOption};

fn option(id: &str, category: OptionCategory) -> ConfigOption {
    ConfigOption {
        category,
        description: None,
        id: id.into(),
        kind: OptionKind::Select {
            choices: vec![Choice {
                description: None,
                group: None,
                id: "a".into(),
                name: "A".into(),
            }],
            current: "a".into(),
        },
        name: id.into(),
    }
}

#[test]
fn only_settings_without_a_button_go_behind_the_more_button() {
    let mut transcript = Transcript::new("Claude");
    transcript.options = vec![
        option("model", OptionCategory::Model),
        option("effort", OptionCategory::ThoughtLevel),
        option("variant", OptionCategory::ModelConfig),
        option("second-model", OptionCategory::Model),
        option("allow_all", OptionCategory::Other),
    ];
    let ids: Vec<_> = others(&transcript)
        .into_iter()
        .map(|option| option.id.as_str())
        .collect();
    assert_eq!(ids, ["variant", "second-model", "allow_all"]);
}

#[test]
fn the_context_meter_leaves_out_the_cost() {
    let usage = Usage {
        cost: Some(sheprd_chat_protocol::Cost {
            currency: "USD".into(),
            micros: 1_890_000,
        }),
        size: 1_000_000,
        used: 109_100,
    };
    let label = usage_label(&usage);
    assert!(!label.contains('$'), "{label}");
    assert_eq!(
        label,
        format!(
            "{} / {}",
            display::tokens(109_100),
            display::tokens(1_000_000)
        )
    );
}

#[test]
fn settings_at_hand_have_marks_of_their_own() {
    let marks: Vec<_> = AT_HAND.into_iter().map(category_icon).collect();
    for (index, mark) in marks.iter().enumerate() {
        assert!(!marks[..index].contains(mark), "{mark} is used twice");
        assert!(
            matches!(crate::icons::Icons.load(mark), Ok(Some(_))),
            "{mark} is not embedded"
        );
    }
}
