use super::*;
use herdr_chat_protocol::{Status, Tokens, Turn, TurnEnd, TurnSummary};

fn event(payload: Value) -> Value {
    json!({"type":"event_msg","payload":payload})
}

fn start(id: &str) -> Value {
    event(json!({"type":"task_started","turn_id":id,"started_at":1700000000}))
}

fn end(id: &str) -> Value {
    event(json!({"type":"task_complete","turn_id":id,"duration_ms":1250}))
}

fn usage(input: u64, output: u64) -> Value {
    event(json!({"type":"token_count","info":{"total_token_usage":{
        "input_tokens":input,"output_tokens":output,"cached_input_tokens":10,
        "reasoning_output_tokens":2,"total_tokens":input+output
    }}}))
}

fn summaries(transcript: &Transcript) -> Vec<&TurnSummary> {
    transcript
        .entries
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::Turn(summary) => Some(summary),
            _ => None,
        })
        .collect()
}

#[test]
fn explicit_open_turn_has_a_clock_but_never_claims_process_liveness() {
    let transcript = parse(TranscriptAgent::Codex, &lines(&[start("one")]), 100);
    assert_eq!(
        transcript.turn,
        Some(Turn {
            started_ms: 1700000000000
        })
    );
    assert_eq!(transcript.status, Status::Starting);
    assert!(transcript.read_only && transcript.permissions.is_empty());
}

#[test]
fn matching_completion_is_final_and_replays_cannot_reopen_or_duplicate_it() {
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[
            start("one"),
            end("other"),
            end("one"),
            end("one"),
            start("one"),
        ]),
        0,
    );
    assert!(transcript.turn.is_none());
    assert_eq!(
        summaries(&transcript),
        [&TurnSummary {
            duration_ms: 1250,
            end: TurnEnd::Done,
            tokens: None,
        }]
    );
    let open = parse(
        TranscriptAgent::Codex,
        &lines(&[start("one"), start("two"), end("one")]),
        0,
    );
    assert!(open.turn.is_some());
    assert!(summaries(&open).is_empty());
}

#[test]
fn cumulative_usage_requires_valid_counts_on_both_sides_of_this_turn() {
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[usage(100, 20), start("one"), usage(120, 27), end("one")]),
        0,
    );
    assert_eq!(
        summaries(&transcript)[0].tokens,
        Some(Tokens {
            cached: 0,
            input: 20,
            output: 7,
            thought: 0,
            total: 27,
        })
    );
    for records in [
        vec![start("one"), usage(120, 27), end("one")],
        vec![
            usage(100, 20),
            start("one"),
            usage(1, 1),
            usage(120, 27),
            end("one"),
        ],
        vec![usage(100, 20), start("one"), end("one")],
    ] {
        assert!(
            summaries(&parse(TranscriptAgent::Codex, &lines(&records), 0))[0]
                .tokens
                .is_none()
        );
    }
}

#[test]
fn malformed_counts_do_not_become_a_reported_turn_total() {
    let mut invalid = usage(120, 27);
    invalid["payload"]["info"]["total_token_usage"]["output_tokens"] = json!(-1);
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[
            usage(100, 20),
            start("one"),
            invalid,
            usage(120, 27),
            end("one"),
        ]),
        0,
    );
    assert!(summaries(&transcript)[0].tokens.is_none());
}

#[test]
fn explicit_abort_and_failure_do_not_look_like_success() {
    for (payload, expected) in [
        (
            json!({"type":"turn_aborted","turn_id":"one","duration_ms":4,"reason":"interrupted"}),
            TurnEnd::Cancelled,
        ),
        (
            json!({"type":"turn_aborted","turn_id":"one","duration_ms":4,"reason":"budget_limited"}),
            TurnEnd::Limit,
        ),
        (
            json!({"type":"task_complete","turn_id":"one","duration_ms":4,"error":{"message":"failed"}}),
            TurnEnd::Failed,
        ),
    ] {
        let transcript = parse(
            TranscriptAgent::Codex,
            &lines(&[start("one"), event(payload)]),
            0,
        );
        assert_eq!(summaries(&transcript)[0].end, expected);
        assert!(transcript.turn.is_none());
    }
}

#[test]
fn old_records_use_structured_timestamps_without_guessing_invalid_durations() {
    let first = json!({"type":"event_msg","timestamp":"2026-10-25T12:00:00.125Z",
        "payload":{"type":"task_started","turn_id":"one"}});
    let last = json!({"type":"event_msg","timestamp":"2026-10-25T12:00:01.375Z",
        "payload":{"type":"task_complete","turn_id":"one"}});
    assert_eq!(
        summaries(&parse(
            TranscriptAgent::Codex,
            &lines(&[first.clone(), last.clone()]),
            0
        ))[0]
            .duration_ms,
        1250
    );
    for bad in [json!(-1), json!("1250"), json!(u64::MAX)] {
        let mut invalid = last.clone();
        invalid["payload"]["duration_ms"] = bad;
        let transcript = parse(TranscriptAgent::Codex, &lines(&[first.clone(), invalid]), 0);
        assert!(transcript.turn.is_none() && summaries(&transcript).is_empty());
    }
}

#[test]
fn ambiguity_in_the_retained_history_clears_the_open_clock() {
    for boundary in [
        json!({"type":"compacted","payload":{}}),
        event(json!({"type":"context_compacted"})),
        event(json!({"type":"thread_rolled_back"})),
        json!({"type":"turn_context","payload":{"turn_id":"other"}}),
        json!({"type":"turn_context","payload":{}}),
        json!({"type":"session_meta","payload":{"id":"another-session"}}),
    ] {
        let transcript = parse(
            TranscriptAgent::Codex,
            &lines(&[start("one"), boundary, end("one")]),
            0,
        );
        assert!(transcript.turn.is_none() && summaries(&transcript).is_empty());
    }
    for bad in [
        b"not-json\n".to_vec(),
        lines(&[json!({"too_large":"x".repeat(RECORD)})]),
    ] {
        let mut bytes = lines(&[start("one")]);
        bytes.extend(bad);
        bytes.extend(lines(&[end("one")]));
        let transcript = parse(TranscriptAgent::Codex, &bytes, 0);
        assert!(transcript.turn.is_none() && summaries(&transcript).is_empty());
    }
}

#[test]
fn identifiers_and_seen_turns_are_bounded_without_reusing_old_clocks() {
    let transcript = parse(
        TranscriptAgent::Codex,
        &lines(&[start("one"), start(&"x".repeat(513))]),
        0,
    );
    assert!(transcript.turn.is_none());
    let records = (0..=MAX_ENTRIES)
        .map(|index| start(&format!("turn-{index}")))
        .collect::<Vec<_>>();
    assert!(
        parse(TranscriptAgent::Codex, &lines(&records), 0)
            .turn
            .is_none()
    );
}

#[test]
fn claude_duration_is_reported_without_inventing_success_counts_or_a_live_start() {
    let record =
        json!({"type":"system","subtype":"turn_duration","durationMs":12000,"uuid":"duration-one"});
    let transcript = parse(
        TranscriptAgent::Claude,
        &lines(&[record.clone(), record.clone()]),
        0,
    );
    assert_eq!(
        summaries(&transcript),
        [&TurnSummary {
            duration_ms: 12000,
            end: TurnEnd::Other,
            tokens: None
        }]
    );
    assert!(transcript.turn.is_none() && transcript.usage.is_none());
    assert_eq!(transcript.status, Status::Starting);
    for (field, value) in [
        ("isSidechain", json!(true)),
        ("isSidechain", json!("false")),
        ("durationMs", json!(-1)),
    ] {
        let mut invalid = record.clone();
        invalid[field] = value;
        assert!(summaries(&parse(TranscriptAgent::Claude, &lines(&[invalid]), 0)).is_empty());
    }
}
