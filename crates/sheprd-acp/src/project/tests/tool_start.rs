//! A call keeps when it was first seen running, for the working row's step
//! timer.

use super::*;

fn apply(projector: &mut Projector, transcript: &mut Transcript, value: Value, now: u64) {
    for change in projector.update(transcript, update(value), now) {
        transcript.apply(&change).unwrap();
    }
}

fn started(transcript: &Transcript) -> Option<u64> {
    transcript
        .entries
        .iter()
        .find_map(|entry| match &entry.kind {
            EntryKind::Tool(tool) => tool.started_ms,
            _ => None,
        })
}

#[test]
fn a_call_keeps_the_time_it_was_first_seen_through_its_updates() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    apply(
        &mut projector,
        &mut transcript,
        json!({
            "sessionUpdate": "tool_call", "toolCallId": "c", "title": "ls",
            "status": "pending",
        }),
        1_000,
    );
    assert_eq!(started(&transcript), Some(1_000));
    for (status, now) in [("in_progress", 2_000), ("completed", 3_000)] {
        apply(
            &mut projector,
            &mut transcript,
            json!({
                "sessionUpdate": "tool_call_update", "toolCallId": "c",
                "status": status,
            }),
            now,
        );
        assert_eq!(started(&transcript), Some(1_000));
    }
}

#[test]
fn a_call_replayed_already_finished_has_no_start() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    apply(
        &mut projector,
        &mut transcript,
        json!({
            "sessionUpdate": "tool_call", "toolCallId": "c", "title": "ls",
            "status": "completed",
        }),
        1_000,
    );
    assert_eq!(started(&transcript), None);
}
