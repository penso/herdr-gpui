use super::*;

#[test]
fn durations_read_at_a_glance() {
    assert_eq!(crate::display::duration(950), "950ms");
    assert_eq!(crate::display::duration(12_400), "12s");
    assert_eq!(crate::display::duration(184_000), "3m 4s");
    assert_eq!(crate::display::duration(3_720_000), "1h 2m");
}

#[test]
fn token_counts_shorten() {
    assert_eq!(crate::display::tokens(950), "950");
    assert_eq!(crate::display::tokens(1000), "1k");
    assert_eq!(crate::display::tokens(18_640), "18.6k");
    assert_eq!(crate::display::tokens(1_200_000), "1.2M");
}

#[test]
fn costs_show_their_currency() {
    let cost = |currency: &str, micros| Cost {
        currency: currency.into(),
        micros,
    };
    assert_eq!(crate::display::cost(&cost("USD", 420_000)), "$0.42");
    assert_eq!(crate::display::cost(&cost("USD", 1_500)), "$0.0015");
    assert_eq!(crate::display::cost(&cost("EUR", 2_000_000)), "2.00 EUR");
}

#[test]
fn a_turn_summary_names_how_it_ended() {
    let summary = TurnSummary {
        duration_ms: 12_000,
        ended_ms: None,
        end: TurnEnd::Done,
        tokens: Some(Tokens {
            cached: 0,
            input: 3200,
            output: 450,
            thought: 0,
            total: 3650,
        }),
    };
    assert_eq!(
        summary.describe(),
        "Worked 12s \u{b7} 3.2k in \u{b7} 450 out"
    );
    let cancelled = TurnSummary {
        duration_ms: 3000,
        ended_ms: None,
        end: TurnEnd::Cancelled,
        tokens: None,
    };
    assert_eq!(cancelled.describe(), "Cancelled after 3s");
}

#[test]
fn usage_percent_is_bounded() {
    let usage = |used, size| Usage {
        cost: None,
        size,
        used,
    };
    assert_eq!(usage(50, 200).percent(), 25);
    assert_eq!(usage(500, 200).percent(), 100);
    assert_eq!(usage(5, 0).percent(), 0);
}

#[test]
fn settings_check_the_values_they_take() {
    let model = ConfigOption {
        category: OptionCategory::Model,
        description: None,
        id: "model".into(),
        kind: OptionKind::Select {
            choices: vec![Choice {
                description: None,
                group: None,
                id: "opus".into(),
                name: "Opus".into(),
            }],
            current: "opus".into(),
        },
        name: "Model".into(),
    };
    assert_eq!(model.current_name(), "Opus");
    assert!(model.accepts(&OptionValue::Choice { id: "opus".into() }));
    assert!(!model.accepts(&OptionValue::Choice { id: "haiku".into() }));
    assert!(!model.accepts(&OptionValue::Toggle { value: true }));
    let mut transcript = Transcript::new("Claude");
    transcript
        .apply(&Change::Options {
            options: vec![model],
        })
        .unwrap();
    assert!(transcript.option(OptionCategory::Model).is_some());
    assert!(transcript.option(OptionCategory::ThoughtLevel).is_none());
    // An unknown category from a newer agent still decodes.
    let other: OptionCategory = serde_json::from_value(serde_json::json!("speed")).unwrap();
    assert_eq!(other, OptionCategory::Other);
}
