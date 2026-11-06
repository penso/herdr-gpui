use sheprd_chat_protocol::{OptionCategory, OptionKind, OptionValue, TurnEnd};

use super::*;

fn options() -> Value {
    json!([
        {"id": "model", "name": "Model", "category": "model", "type": "select",
            "currentValue": "opus", "options": [
                {"value": "opus", "name": "Opus"}, {"value": "sonnet", "name": "Sonnet"}]},
        {"id": "effort", "name": "Thinking", "category": "thought_level", "type": "select",
            "currentValue": "high", "options": [{"group": "levels", "name": "Levels",
                "options": [{"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]}]},
        {"id": "fast", "name": "Fast", "type": "boolean", "currentValue": false},
        {"id": "future", "name": "Future", "type": "slider", "min": 0}
    ])
}

/// A session opened with the settings above.
fn configured() -> Harness {
    let mut harness = harness_with(config());
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(id, json!({"protocolVersion": 1, "agentCapabilities": {}}));
    let id = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.respond(id, json!({"sessionId": "s1", "configOptions": options()}));
    harness
}

#[test]
fn settings_arrive_with_the_session_and_unknown_kinds_are_skipped() {
    let harness = configured();
    assert_eq!(harness.transcript().status, Status::Idle);
    let options = &harness.transcript().options;
    let ids: Vec<_> = options.iter().map(|option| option.id.as_str()).collect();
    assert_eq!(ids, ["model", "effort", "fast"]);
    let thinking = harness
        .transcript()
        .option(OptionCategory::ThoughtLevel)
        .unwrap();
    let OptionKind::Select { choices, current } = &thinking.kind else {
        panic!("a select");
    };
    assert_eq!(current, "high");
    assert_eq!(choices[0].group.as_deref(), Some("Levels"));
    assert_eq!(
        harness
            .transcript()
            .option(OptionCategory::Model)
            .unwrap()
            .current_name(),
        "Opus"
    );
}

#[test]
fn changing_a_setting_asks_the_agent_and_takes_its_answer() {
    let mut harness = configured();
    harness.command(
        0,
        Command::SetOption {
            id: "model".into(),
            value: OptionValue::Choice { id: "haiku".into() },
        },
    );
    assert_eq!(
        harness.session.handled.back().unwrap().1,
        Some(Rejection::UnknownSetting)
    );
    harness.command(
        0,
        Command::SetOption {
            id: "model".into(),
            value: OptionValue::Choice {
                id: "sonnet".into(),
            },
        },
    );
    let request = harness.last_request("session/set_config_option");
    assert_eq!(request["params"]["configId"], "model");
    assert_eq!(request["params"]["value"], "sonnet");
    let mut answered = options();
    answered[0]["currentValue"] = json!("sonnet");
    harness.respond(
        request["id"].as_i64().unwrap(),
        json!({"configOptions": answered}),
    );
    assert_eq!(
        harness
            .transcript()
            .option(OptionCategory::Model)
            .unwrap()
            .current_name(),
        "Sonnet"
    );
}

#[test]
fn the_agent_can_change_settings_on_its_own() {
    let mut harness = configured();
    let mut changed = options();
    changed[2]["currentValue"] = json!(true);
    harness.update(json!({"sessionUpdate": "config_option_update", "configOptions": changed}));
    let fast = harness
        .transcript()
        .options
        .iter()
        .find(|option| option.id == "fast")
        .unwrap();
    assert_eq!(fast.kind, OptionKind::Toggle { current: true });
}

#[test]
fn a_turn_is_timed_and_summarised_with_its_tokens() {
    let mut harness = ready();
    let id = prompt(&mut harness, 0, "go");
    assert!(harness.transcript().turn.is_some());
    harness.respond(
        id,
        json!({"stopReason": "end_turn", "usage": {"totalTokens": 120, "inputTokens": 100,
            "outputTokens": 20, "thoughtTokens": 5}}),
    );
    assert!(harness.transcript().turn.is_none());
    let EntryKind::Turn(summary) = &harness.transcript().entries.last().unwrap().kind else {
        panic!("a turn summary");
    };
    assert_eq!(summary.end, TurnEnd::Done);
    let tokens = summary.tokens.unwrap();
    assert_eq!((tokens.input, tokens.output, tokens.thought), (100, 20, 5));

    let id = prompt(&mut harness, 0, "again");
    harness.respond(id, json!({"stopReason": "cancelled"}));
    let EntryKind::Turn(summary) = &harness.transcript().entries.last().unwrap().kind else {
        panic!("a turn summary");
    };
    assert_eq!(summary.end, TurnEnd::Cancelled);
    assert_eq!(summary.tokens, None);
}

#[test]
fn usage_carries_its_cost() {
    let mut harness = ready();
    harness.update(
        json!({"sessionUpdate": "usage_update", "used": 50000, "size": 200000,
        "cost": {"amount": 0.42, "currency": "USD"}}),
    );
    let usage = harness.transcript().usage.clone().unwrap();
    assert_eq!(usage.percent(), 25);
    assert_eq!(usage.cost.unwrap().micros, 420_000);
}
