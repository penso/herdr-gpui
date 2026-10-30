use super::*;

fn activity(call: &str, kind: ActivityKind, state: ActivityState) -> Activity {
    Activity {
        call_id: call.to_owned(),
        detail: None,
        finished_ms: (state != ActivityState::Running).then_some(2_000),
        kind,
        label: format!("{call} label"),
        started_ms: 1_000,
        state,
    }
}

fn record(transcript: &mut Transcript, activity: Activity) -> Result<()> {
    transcript.apply(&Change::Activity { activity })
}

#[test]
fn an_activity_is_added_then_replaced_by_its_call_id() {
    let mut transcript = Transcript::new("Claude");
    record(
        &mut transcript,
        activity("task", ActivityKind::Subagent, ActivityState::Running),
    )
    .unwrap();
    record(
        &mut transcript,
        activity("shell", ActivityKind::Shell, ActivityState::Running),
    )
    .unwrap();
    assert_eq!(
        transcript.running(),
        Running {
            other: 0,
            shells: 1,
            subagents: 1
        }
    );
    record(
        &mut transcript,
        activity("task", ActivityKind::Subagent, ActivityState::Finished),
    )
    .unwrap();
    assert_eq!(transcript.activities.len(), 2);
    assert_eq!(transcript.activities[0].call_id, "task", "order is kept");
    assert_eq!(transcript.running().total(), 1);
    assert_eq!(
        transcript.activity("task").unwrap().state,
        ActivityState::Finished
    );
}

#[test]
fn a_full_list_drops_the_oldest_ended_activity() {
    let mut transcript = Transcript::new("Claude");
    record(
        &mut transcript,
        activity("first", ActivityKind::Shell, ActivityState::Running),
    )
    .unwrap();
    record(
        &mut transcript,
        activity("ended", ActivityKind::Shell, ActivityState::Killed),
    )
    .unwrap();
    for index in 2..MAX_ACTIVITIES {
        record(
            &mut transcript,
            activity(
                &format!("call-{index}"),
                ActivityKind::Shell,
                ActivityState::Running,
            ),
        )
        .unwrap();
    }
    record(
        &mut transcript,
        activity("new", ActivityKind::Shell, ActivityState::Running),
    )
    .unwrap();
    assert_eq!(transcript.activities.len(), MAX_ACTIVITIES);
    assert!(transcript.activity("ended").is_none());
    assert!(transcript.activity("first").is_some(), "running ones stay");

    let error = record(
        &mut transcript,
        activity("one-too-many", ActivityKind::Shell, ActivityState::Running),
    )
    .unwrap_err();
    assert!(matches!(error, Error::ActivityLimit));
    assert_eq!(transcript.activities.len(), MAX_ACTIVITIES);
    assert!(transcript.activity("one-too-many").is_none());
}

#[test]
fn oversized_activity_text_is_refused() {
    let mut transcript = Transcript::new("Claude");
    let long = "x".repeat(MAX_ACTIVITY_TEXT + 1);
    for oversized in [
        Activity {
            call_id: long.clone(),
            ..activity("a", ActivityKind::Shell, ActivityState::Running)
        },
        Activity {
            label: long.clone(),
            ..activity("a", ActivityKind::Shell, ActivityState::Running)
        },
        Activity {
            detail: Some(long.clone()),
            ..activity("a", ActivityKind::Shell, ActivityState::Running)
        },
    ] {
        let error = record(&mut transcript, oversized).unwrap_err();
        assert!(matches!(error, Error::ActivityText));
    }
    assert!(transcript.activities.is_empty());
    // A replacement that does not fit leaves the old one alone too.
    record(
        &mut transcript,
        activity("a", ActivityKind::Shell, ActivityState::Running),
    )
    .unwrap();
    let error = record(
        &mut transcript,
        Activity {
            label: long,
            ..activity("a", ActivityKind::Shell, ActivityState::Failed)
        },
    )
    .unwrap_err();
    assert!(matches!(error, Error::ActivityText));
    assert!(transcript.activity("a").unwrap().is_running());
}

#[test]
fn activities_read_from_older_and_newer_programs() {
    // An older program sends no activities and no parents.
    let old: Transcript = serde_json::from_str(r#"{"agent": "Claude"}"#).unwrap();
    assert!(old.activities.is_empty());
    let tool: Tool =
        serde_json::from_str(r#"{"call_id": "c", "title": "Run", "kind": "execute"}"#).unwrap();
    assert_eq!(tool.parent, None);
    // A newer program may name kinds and outcomes this one does not know.
    let change: Change = serde_json::from_str(
        r#"{"change": "activity", "activity": {
            "call_id": "c", "label": "x", "started_ms": 1,
            "kind": "browser", "state": "paused_for_review"}}"#,
    )
    .unwrap();
    let Change::Activity { activity } = change else {
        panic!("expected an activity change");
    };
    assert_eq!(activity.kind, ActivityKind::Other);
    assert_eq!(activity.state, ActivityState::Finished);
    let subagent: Tool =
        serde_json::from_str(r#"{"call_id": "c", "title": "Explore", "kind": "subagent"}"#)
            .unwrap();
    assert_eq!(subagent.kind, ToolKind::Subagent);
}

#[test]
fn malformed_activities_do_not_decode() {
    for malformed in [
        // No call id.
        r#"{"change": "activity", "activity": {"label": "x", "started_ms": 1}}"#,
        // A start time that is not a number.
        r#"{"change": "activity", "activity": {"call_id": "c", "label": "x", "started_ms": "now"}}"#,
        // No activity at all.
        r#"{"change": "activity"}"#,
    ] {
        assert!(
            serde_json::from_str::<Change>(malformed).is_err(),
            "{malformed}"
        );
    }
}

#[test]
fn an_empty_activity_list_is_left_out_of_a_snapshot() {
    let json = serde_json::to_value(Transcript::new("Claude")).unwrap();
    assert!(json.get("activities").is_none());
    let mut transcript = Transcript::new("Claude");
    record(
        &mut transcript,
        activity("c", ActivityKind::Subagent, ActivityState::Running),
    )
    .unwrap();
    let round: Transcript =
        serde_json::from_value(serde_json::to_value(&transcript).unwrap()).unwrap();
    assert_eq!(round, transcript);
}
