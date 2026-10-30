use herdr_chat_protocol::{Activity, ActivityKind, ActivityState};

use super::*;

fn activity(call: &str, state: ActivityState) -> Activity {
    Activity {
        call_id: call.into(),
        detail: None,
        finished_ms: (state != ActivityState::Running).then_some(2),
        kind: ActivityKind::Shell,
        label: "npm run dev".into(),
        started_ms: 1,
        state,
    }
}

#[test]
fn new_and_ended_activities_reach_clients_as_changes() {
    let previous = transcript(&["one"]);
    let mut next = previous.clone();
    next.activities = vec![activity("a", ActivityState::Running)];
    let started = changes(&previous, &next).unwrap();
    assert!(matches!(&started[..], [Change::Activity { .. }]));
    let mut ended = next.clone();
    ended.activities = vec![activity("a", ActivityState::Finished)];
    let mut actual = next.clone();
    for change in changes_or_panic(&next, &ended) {
        actual.apply(&change).unwrap();
    }
    assert_eq!(actual, ended);
    assert!(changes(&ended, &ended).unwrap().is_empty());
}

#[test]
fn an_activity_that_disappears_needs_a_snapshot() {
    let mut previous = transcript(&["one"]);
    previous.activities = vec![activity("a", ActivityState::Finished)];
    let next = transcript(&["one"]);
    assert!(changes(&previous, &next).is_none());
}

fn changes_or_panic(previous: &Transcript, next: &Transcript) -> Vec<Change> {
    changes(previous, next).unwrap()
}
