//! The payloads must match the fixtures the Swift package decodes, so the
//! two sides cannot drift apart.

use super::*;
use crate::apns::payload::MAX_PAYLOAD;
use serde_json::Value;

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../ios/HerdrRemoteCore/Tests/HerdrRemoteCoreTests/Fixtures/",
            $name
        ))
    };
}

fn base_state() -> ContentState {
    ContentState {
        cost: None,
        phase: Phase::Working,
        plan_done: None,
        plan_total: None,
        question: None,
        started_at: 1_799_999_990,
        step: "Working".to_owned(),
        summary: None,
        waiting: 0,
        working: 1,
    }
}

fn json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

#[test]
fn start_matches_the_swift_fixture() {
    let live = Live {
        event: Event::Start {
            alert: Alert {
                body: "herdr-gpui \u{b7} remote plan".to_owned(),
                title: "Claude is working".to_owned(),
            },
            attributes: Attributes {
                agent: "Claude".to_owned(),
                chat_title: "remote plan".to_owned(),
                computer: "studio".to_owned(),
                computer_id: "AB".repeat(32),
                pane: "w1-p3".to_owned(),
                workspace: "herdr-gpui".to_owned(),
            },
        },
        pane: "w1-p3".to_owned(),
        state: base_state(),
    };
    let payload = json(&live.payload(1_800_000_000).unwrap());
    assert_eq!(payload, json(fixture!("start.json").as_bytes()));
    assert_eq!(payload["aps"]["attributes-type"], ATTRIBUTES_TYPE);
}

#[test]
fn question_update_matches_the_swift_fixture() {
    let live = Live {
        event: Event::Update {
            alert: Some(Alert {
                body: "herdr-gpui \u{b7} remote plan".to_owned(),
                title: "Claude needs an answer".to_owned(),
            }),
        },
        pane: "w1-p3".to_owned(),
        state: ContentState {
            cost: Some("$0.34".to_owned()),
            phase: Phase::Question,
            plan_done: Some(2),
            plan_total: Some(4),
            question: Some("Run just ci".to_owned()),
            step: "Run the tests".to_owned(),
            waiting: 1,
            ..base_state()
        },
    };
    assert!(live.urgent());
    let payload = json(&live.payload(1_800_000_060).unwrap());
    assert_eq!(payload, json(fixture!("update.json").as_bytes()));
}

#[test]
fn end_matches_the_swift_fixture() {
    let live = Live {
        event: Event::End {
            dismissal_s: 1_800_001_262,
        },
        pane: "w1-p3".to_owned(),
        state: ContentState {
            cost: Some("$0.48".to_owned()),
            phase: Phase::Finished,
            step: "Run the tests".to_owned(),
            summary: Some("Worked 6m 2s \u{b7} $0.48".to_owned()),
            working: 0,
            ..base_state()
        },
    };
    assert!(!live.urgent());
    let payload = json(&live.payload(1_800_000_362).unwrap());
    assert_eq!(payload, json(fixture!("end.json").as_bytes()));
}

#[test]
fn update_without_alert_has_only_the_state() {
    let live = Live {
        event: Event::Update { alert: None },
        pane: "w1-p3".to_owned(),
        state: base_state(),
    };
    let payload = json(&live.payload(5).unwrap());
    let aps = payload["aps"].as_object().unwrap();
    let mut keys: Vec<&str> = aps.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["content-state", "event", "timestamp"]);
}

#[test]
fn content_state_round_trips_and_tolerates_missing_optionals() {
    let state: ContentState = serde_json::from_str(
        r#"{"phase":"failed","startedAt":3,"step":"x","summary":"Claude stopped"}"#,
    )
    .unwrap();
    assert_eq!(state.phase, Phase::Failed);
    assert_eq!(state.working, 0);
    let back: ContentState = serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
    assert_eq!(back, state);
}

#[test]
fn oversized_payload_is_refused() {
    let live = Live {
        event: Event::Update { alert: None },
        pane: "w1-p3".to_owned(),
        state: ContentState {
            step: "x".repeat(MAX_PAYLOAD),
            ..base_state()
        },
    };
    let error = live.payload(5).unwrap_err();
    assert!(matches!(error, Error::PayloadTooLarge { .. }), "{error:?}");
}
