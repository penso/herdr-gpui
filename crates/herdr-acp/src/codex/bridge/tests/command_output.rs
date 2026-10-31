use super::*;
use crate::project::{MAX_ENTRY_TEXT, Projector};
use herdr_chat_protocol::{EntryKind, ToolContent, ToolStatus, Transcript};

fn command(id: &str, text: &str) -> Value {
    json!({"type": "commandExecution", "id": id, "command": text, "status": "inProgress"})
}

fn item_event(bridge: &mut Bridge, method: &str, item: Value) -> Vec<Out> {
    notify(
        bridge,
        method,
        json!({"threadId": THREAD, "turnId": "turn-1", "item": item}),
    )
}

fn delta(bridge: &mut Bridge, id: &str, text: &str) -> Vec<Out> {
    let mut outs = notify(
        bridge,
        "item/commandExecution/outputDelta",
        json!({"threadId": THREAD, "turnId": "turn-1", "itemId": id, "delta": text}),
    );
    outs.extend(bridge.tick());
    outs
}

fn content(out: &Out) -> Vec<&str> {
    update(out)["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["content"]["text"].as_str().unwrap())
        .collect()
}

fn apply(projector: &mut Projector, transcript: &mut Transcript, outs: &[Out]) {
    for out in outs {
        let event = serde_json::from_value(update(out).clone()).unwrap();
        for change in projector.update(transcript, event, 0) {
            transcript.apply(&change).unwrap();
        }
    }
}

#[test]
fn output_reaches_the_existing_tool_before_completion_and_final_text_replaces_it() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    let mut item = command("cmd", "printf first\nprintf second");
    apply(
        &mut projector,
        &mut transcript,
        &item_event(&mut bridge, "item/started", item.clone()),
    );
    for text in ["café\n", "ha", "ha"] {
        apply(
            &mut projector,
            &mut transcript,
            &delta(&mut bridge, "cmd", text),
        );
    }
    assert_eq!(transcript.entries.len(), 1);
    let entry_id = transcript.entries[0].id;
    let EntryKind::Tool(tool) = &transcript.entries[0].kind else {
        panic!("expected a tool");
    };
    assert_eq!(tool.status, ToolStatus::InProgress);
    assert_eq!(
        tool.content,
        [
            ToolContent::Text {
                text: "printf first\nprintf second".to_owned()
            },
            ToolContent::Text {
                text: "café\nhaha".to_owned()
            },
        ]
    );
    item["status"] = json!("failed");
    item["aggregatedOutput"] = json!("authoritative error\n");
    apply(
        &mut projector,
        &mut transcript,
        &item_event(&mut bridge, "item/completed", item),
    );
    assert_eq!(transcript.entries.len(), 1);
    assert_eq!(transcript.entries[0].id, entry_id);
    let EntryKind::Tool(tool) = &transcript.entries[0].kind else {
        panic!("expected a tool");
    };
    assert_eq!(tool.status, ToolStatus::Failed);
    assert_eq!(
        tool.content,
        [
            ToolContent::Text {
                text: "printf first\nprintf second".to_owned()
            },
            ToolContent::Text {
                text: "authoritative error\n".to_owned()
            },
        ]
    );
    assert!(!bridge.commands.active.contains("cmd"));
    assert!(delta(&mut bridge, "cmd", "late").is_empty());
}

#[test]
fn final_aggregate_is_neither_duplicated_nor_replaced_by_stale_partial_output() {
    for aggregate in [json!("partial"), json!(""), Value::Null] {
        let mut bridge = opened();
        prompt(&mut bridge, 2);
        let mut item = command("cmd", "echo test");
        item_event(&mut bridge, "item/started", item.clone());
        assert_eq!(
            content(&delta(&mut bridge, "cmd", "partial")[0]),
            ["partial"]
        );
        item["status"] = json!("completed");
        item["aggregatedOutput"] = aggregate.clone();
        let final_update = item_event(&mut bridge, "item/completed", item);
        assert_eq!(update(&final_update[0])["status"], "completed");
        let expected: Vec<_> = aggregate
            .as_str()
            .filter(|text| !text.is_empty())
            .into_iter()
            .collect();
        assert_eq!(content(&final_update[0]), expected);
        assert!(delta(&mut bridge, "cmd", "late").is_empty());
    }
}

#[test]
fn malformed_unknown_and_wrong_turn_output_cannot_change_a_live_command() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let item = command("cmd", "echo test");
    assert!(delta(&mut bridge, "cmd", "before start").is_empty());
    item_event(&mut bridge, "item/started", item.clone());
    let valid = json!({"threadId": THREAD, "turnId": "turn-1", "itemId": "cmd", "delta": "bad"});
    for (field, bad) in [
        ("threadId", json!("another-thread")),
        ("turnId", json!("another-turn")),
        ("itemId", json!("unknown")),
        ("itemId", json!("")),
        ("delta", json!("")),
        ("delta", json!(42)),
        ("delta", Value::Null),
    ] {
        let mut params = valid.clone();
        params[field] = bad;
        assert!(notify(&mut bridge, "item/commandExecution/outputDelta", params).is_empty());
    }
    for field in ["threadId", "turnId", "itemId", "delta"] {
        let mut params = valid.clone();
        params.as_object_mut().unwrap().remove(field);
        assert!(notify(&mut bridge, "item/commandExecution/outputDelta", params).is_empty());
    }
    assert_eq!(content(&delta(&mut bridge, "cmd", "first")[0]), ["first"]);
    assert!(item_event(&mut bridge, "item/started", item.clone()).is_empty());
    assert!(
        notify(
            &mut bridge,
            "item/completed",
            json!({"threadId": THREAD, "turnId": "old-turn", "item": item}),
        )
        .is_empty()
    );
    assert_eq!(
        content(&delta(&mut bridge, "cmd", " second")[0]),
        ["first second"]
    );
}

#[test]
fn oversized_output_is_utf8_bounded_and_final_output_uses_the_same_limit() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let mut item = command("cmd", "echo test");
    item_event(&mut bridge, "item/started", item.clone());
    let prefix = "a".repeat(MAX_ENTRY_TEXT - 1);
    delta(&mut bridge, "cmd", &prefix);
    let cut = delta(&mut bridge, "cmd", "é🙂");
    let expected = crate::project::bounded(format!("{prefix}é🙂"));
    assert_eq!(content(&cut[0]), [expected.as_str()]);
    assert!(expected.len() <= MAX_ENTRY_TEXT);
    assert!(expected.ends_with("… (truncated)"));
    assert!(delta(&mut bridge, "cmd", "later output").is_empty());
    item["status"] = json!("completed");
    item["aggregatedOutput"] = json!("🙂".repeat(MAX_ENTRY_TEXT));
    let done = item_event(&mut bridge, "item/completed", item);
    let text = content(&done[0])[0];
    assert!(text.len() <= MAX_ENTRY_TEXT);
    assert!(text.starts_with("🙂"));
    assert!(text.ends_with("… (truncated)"));
}

#[test]
fn concurrent_buffers_are_bounded_without_losing_final_results() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    for id in 0..17 {
        assert_eq!(
            item_event(
                &mut bridge,
                "item/started",
                command(&id.to_string(), "test")
            )
            .len(),
            1
        );
    }
    assert_eq!(
        (0..17)
            .filter(|id| bridge.commands.active.contains(&id.to_string()))
            .count(),
        16
    );
    assert!(delta(&mut bridge, "16", "unbuffered").is_empty());
    let mut done = command("16", "test");
    done["status"] = json!("completed");
    done["aggregatedOutput"] = json!("final result");
    assert_eq!(
        content(&item_event(&mut bridge, "item/completed", done.clone())[0]),
        ["final result"]
    );
    done["id"] = json!("0");
    item_event(&mut bridge, "item/completed", done);
    item_event(&mut bridge, "item/started", command("next", "test"));
    assert_eq!(
        content(&delta(&mut bridge, "next", "streamed")[0]),
        ["streamed"]
    );
    let oversized_id = "x".repeat(1025);
    item_event(&mut bridge, "item/started", command(&oversized_id, "test"));
    assert!(delta(&mut bridge, &oversized_id, "ignored").is_empty());
    assert_eq!(
        (0..17)
            .filter(|id| bridge.commands.active.contains(&id.to_string()))
            .count(),
        15
    );
    assert!(bridge.commands.active.contains("next"));
}

#[test]
fn ending_or_replacing_a_turn_releases_buffers_and_old_deltas_stay_inert() {
    for status in ["completed", "failed", "interrupted"] {
        let mut bridge = opened();
        prompt(&mut bridge, 2);
        item_event(&mut bridge, "item/started", command("cmd", "test"));
        delta(&mut bridge, "cmd", "old output");
        // A duplicate turn-start notification does not discard live output.
        notify(
            &mut bridge,
            "turn/started",
            json!({"threadId": THREAD, "turn": {"id": "turn-1"}}),
        );
        assert_eq!(
            content(&delta(&mut bridge, "cmd", " continues")[0]),
            ["old output continues"]
        );
        notify(
            &mut bridge,
            "turn/completed",
            json!({"threadId": THREAD, "turn": {"id": "turn-1", "status": status}}),
        );
        assert!(!bridge.commands.active.contains("cmd"));
        assert!(delta(&mut bridge, "cmd", "late").is_empty());
        prompt(&mut bridge, 3);
        notify(
            &mut bridge,
            "turn/started",
            json!({"threadId": THREAD, "turn": {"id": "turn-2"}}),
        );
        assert!(delta(&mut bridge, "cmd", "old turn").is_empty());
        notify(
            &mut bridge,
            "item/started",
            json!({"threadId": THREAD, "turnId": "turn-2", "item": command("cmd", "test")}),
        );
        let output = notify(
            &mut bridge,
            "item/commandExecution/outputDelta",
            json!({"threadId": THREAD, "turnId": "turn-2", "itemId": "cmd", "delta": "new output"}),
        );
        assert_eq!(content(&output[0]), ["new output"]);
    }
}

#[test]
fn tiny_deltas_share_a_fixed_flush_deadline_and_only_two_content_replacements() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    item_event(&mut bridge, "item/started", command("cmd", "test"));
    assert!(bridge.next_tick().is_none());
    let event = json!({"threadId": THREAD, "turnId": "turn-1", "itemId": "cmd", "delta": "x"});
    let first = notify(
        &mut bridge,
        "item/commandExecution/outputDelta",
        event.clone(),
    );
    assert_eq!(content(&first[0]), ["x"]);
    assert!(
        bridge.next_tick().is_none(),
        "no timer for already published output"
    );
    assert!(
        notify(
            &mut bridge,
            "item/commandExecution/outputDelta",
            event.clone()
        )
        .is_empty()
    );
    let deadline = bridge.next_tick().unwrap();
    for _ in 2..4096 {
        assert!(
            notify(
                &mut bridge,
                "item/commandExecution/outputDelta",
                event.clone()
            )
            .is_empty()
        );
        assert_eq!(bridge.next_tick(), Some(deadline));
    }
    let flushed = bridge.tick();
    assert_eq!(flushed.len(), 1);
    assert_eq!(content(&flushed[0]), ["x".repeat(4096)]);
    assert!(bridge.next_tick().is_none());
    assert!(bridge.tick().is_empty());
}

#[test]
fn final_completion_supersedes_a_queued_flush_and_replayed_starts_cannot_resurrect_it() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let item = command("cmd", "test");
    item_event(&mut bridge, "item/started", item.clone());
    delta(&mut bridge, "cmd", "first");
    notify(
        &mut bridge,
        "item/commandExecution/outputDelta",
        json!({
            "threadId": THREAD, "turnId": "turn-1", "itemId": "cmd", "delta": " pending",
        }),
    );
    assert!(bridge.next_tick().is_some());
    let mut done = item.clone();
    done["status"] = json!("completed");
    done["aggregatedOutput"] = json!("final");
    let outs = item_event(&mut bridge, "item/completed", done);
    assert_eq!(outs.len(), 1);
    assert_eq!(content(&outs[0]), ["final"]);
    assert!(bridge.next_tick().is_none());
    assert!(bridge.tick().is_empty());
    assert!(item_event(&mut bridge, "item/started", item).is_empty());
    assert!(delta(&mut bridge, "cmd", "late").is_empty());
}

#[test]
fn malformed_command_and_turn_completion_cannot_discard_pending_output() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let item = command("cmd", "test");
    item_event(&mut bridge, "item/started", item.clone());
    delta(&mut bridge, "cmd", "first");
    notify(
        &mut bridge,
        "item/commandExecution/outputDelta",
        json!({
            "threadId": THREAD, "turnId": "turn-1", "itemId": "cmd", "delta": " pending",
        }),
    );
    let valid = json!({"threadId": THREAD, "turnId": "turn-1", "item": item});
    for field in ["threadId", "turnId"] {
        for bad in [None, Some(Value::Null), Some(json!(42)), Some(json!("old"))] {
            let mut params = valid.clone();
            if let Some(bad) = bad {
                params[field] = bad;
            } else {
                params.as_object_mut().unwrap().remove(field);
            }
            assert!(notify(&mut bridge, "item/completed", params).is_empty());
        }
    }
    for params in [
        json!({"threadId": THREAD, "turn": {"id": "old", "status": "completed"}}),
        json!({"turn": {"id": "turn-1", "status": "completed"}}),
        json!({"threadId": THREAD, "turn": {"status": "completed"}}),
    ] {
        assert!(notify(&mut bridge, "turn/completed", params).is_empty());
    }
    assert!(bridge.prompt.is_some());
    let ended = notify(
        &mut bridge,
        "turn/completed",
        json!({
            "threadId": THREAD, "turn": {"id": "turn-1", "status": "interrupted"},
        }),
    );
    assert_eq!(ended.len(), 3);
    assert_eq!(content(&ended[0]), ["first pending"]);
    assert_eq!(
        update(&ended[1])["_meta"]["herdr"]["activity"]["kind"],
        "shell"
    );
    assert_eq!(
        response(&ended[2], 2).as_ref().unwrap()["stopReason"],
        "cancelled"
    );
    assert!(bridge.next_tick().is_none());
}

#[test]
fn a_completion_without_an_id_cannot_finish_a_prompt_awaiting_its_turn() {
    let mut bridge = opened();
    let start = bridge.on_session(request(
        2,
        "session/prompt",
        json!({
            "sessionId": THREAD, "prompt": [{"type": "text", "text": "test"}],
        }),
    ));
    assert!(bridge.turn.is_none());
    assert!(
        notify(
            &mut bridge,
            "turn/completed",
            json!({
                "threadId": THREAD, "turn": {"status": "completed"},
            })
        )
        .is_empty()
    );
    assert!(bridge.prompt.is_some());
    bridge.on_harness(&answer(
        &harness(&start[0])["id"],
        json!({"turn": {"id": "turn-1"}}),
    ));
    let ended = notify(
        &mut bridge,
        "turn/completed",
        json!({
            "threadId": THREAD, "turn": {"id": "turn-1", "status": "completed"},
        }),
    );
    assert_eq!(
        response(&ended[0], 2).as_ref().unwrap()["stopReason"],
        "end_turn"
    );
}
