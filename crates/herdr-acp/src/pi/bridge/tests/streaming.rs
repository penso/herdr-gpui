use super::*;

fn start(bridge: &mut Bridge, id: &str, name: &str, args: Value) -> Vec<Out> {
    bridge.on_harness(
        &json!({"type":"tool_execution_start","toolCallId":id,"toolName":name,"args":args}),
    )
}

fn partial(bridge: &mut Bridge, id: &str, name: &str, text: &str) -> Vec<Out> {
    bridge.on_harness(
        &json!({"type":"tool_execution_update","toolCallId":id,"toolName":name,
        "partialResult":{"content":[{"type":"text","text":text}],"details":{}}}),
    )
}

fn end(bridge: &mut Bridge, id: &str, name: &str, text: &str, failed: bool) -> Vec<Out> {
    bridge.on_harness(
        &json!({"type":"tool_execution_end","toolCallId":id,"toolName":name,
        "result":{"content":[{"type":"text","text":text}]},"isError":failed}),
    )
}

fn texts(out: &Out) -> Vec<&str> {
    update(out)["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|block| block.pointer("/content/text").and_then(Value::as_str))
        .collect()
}

#[test]
fn cumulative_tool_output_is_live_coalesced_and_never_appended_twice() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    start(
        &mut bridge,
        "bash-1",
        "bash",
        json!({"command":"first\nsecond"}),
    );
    let first = partial(&mut bridge, "bash-1", "bash", "one");
    assert_eq!(update(&first[0])["status"], "in_progress");
    assert_eq!(texts(&first[0]), ["first\nsecond", "one"]);
    assert!(bridge.next_tick().is_none());
    assert!(partial(&mut bridge, "bash-1", "bash", "one two").is_empty());
    let deadline = bridge.next_tick();
    assert!(deadline.is_some());
    assert!(partial(&mut bridge, "bash-1", "bash", "one two three").is_empty());
    assert_eq!(bridge.next_tick(), deadline);
    let flushed = bridge.tick();
    assert_eq!(texts(&flushed[0]), ["first\nsecond", "one two three"]);
    assert!(bridge.next_tick().is_none());
    assert!(partial(&mut bridge, "bash-1", "bash", "one two three").is_empty());
    assert!(
        bridge.next_tick().is_none(),
        "duplicate snapshots arm no timer"
    );
}

#[test]
fn final_result_supersedes_pending_partials_and_preserves_original_context() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    start(&mut bridge, "c", "bash", json!({"command":"first\nsecond"}));
    partial(&mut bridge, "c", "bash", "partial");
    partial(&mut bridge, "c", "bash", "queued partial");
    let done = end(&mut bridge, "c", "bash", "authoritative final", false);
    assert_eq!(update(&done[0])["status"], "completed");
    assert_eq!(texts(&done[0]), ["first\nsecond", "authoritative final"]);
    assert!(bridge.tick().is_empty());
    assert!(bridge.next_tick().is_none());
    assert!(partial(&mut bridge, "c", "bash", "late").is_empty());
    assert!(end(&mut bridge, "c", "bash", "duplicate", false).is_empty());
    assert!(start(&mut bridge, "c", "bash", json!({"command":"replayed"})).is_empty());
}

#[test]
fn custom_tool_partials_show_the_latest_result_without_guessing_a_delta_contract() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let started = start(
        &mut bridge,
        "custom",
        "an_extension",
        json!({"anything":true}),
    );
    let call: wire::ToolCall = serde_json::from_value(update(&started[0]).clone()).unwrap();
    assert_eq!(call.kind, wire::ToolKind::Other);
    partial(&mut bridge, "custom", "an_extension", "phase one");
    partial(&mut bridge, "custom", "an_extension", "phase two");
    assert_eq!(texts(&bridge.tick()[0]), ["phase two"]);
}

#[test]
fn unrelated_and_malformed_events_cannot_mutate_an_active_tool() {
    let mut bridge = opened();
    assert!(start(&mut bridge, "c", "bash", json!({})).is_empty());
    prompt(&mut bridge, 2);
    start(&mut bridge, "c", "bash", json!({"command":"work"}));
    for (id, name) in [("unknown", "bash"), ("c", "other")] {
        assert!(partial(&mut bridge, id, name, "not this tool").is_empty());
        assert!(end(&mut bridge, id, name, "not this tool", false).is_empty());
    }
    for result in [
        Value::Null,
        json!({"details":{"progress":42}}),
        json!({"content":"wrong"}),
    ] {
        assert!(bridge.on_harness(&json!({"type":"tool_execution_update","toolCallId":"c","toolName":"bash","partialResult":result})).is_empty());
    }
    assert!(bridge.on_harness(&json!({"type":"tool_execution_end","toolCallId":"c","toolName":"bash","isError":"false"})).is_empty());
    assert!(bridge.next_tick().is_none());
    assert!(!partial(&mut bridge, "c", "bash", "still active").is_empty());
}

#[test]
fn an_explicit_empty_final_clears_partial_output_and_absent_content_keeps_it() {
    for (result, expected) in [
        (json!({"content":[]}), Vec::<&str>::new()),
        (json!({}), vec!["latest"]),
    ] {
        let mut bridge = opened();
        prompt(&mut bridge, 2);
        start(&mut bridge, "c", "bash", json!({"command":"work"}));
        partial(&mut bridge, "c", "bash", "latest");
        let done = bridge.on_harness(&json!({"type":"tool_execution_end","toolCallId":"c","toolName":"bash","result":result,"isError":false}));
        assert_eq!(texts(&done[0]), expected);
    }
}

#[test]
fn streamed_edit_diffs_survive_updates_and_authoritative_failure() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    start(
        &mut bridge,
        "edit",
        "edit",
        json!({"path":"a.rs","edits":[{"oldText":"old","newText":"new"}]}),
    );
    let streaming = partial(&mut bridge, "edit", "edit", "editing");
    assert_eq!(update(&streaming[0])["content"][0]["oldText"], "old");
    assert_eq!(update(&streaming[0])["content"][0]["newText"], "new");
    let done = end(&mut bridge, "edit", "edit", "Operation aborted", true);
    assert_eq!(update(&done[0])["status"], "failed");
    assert_eq!(update(&done[0])["content"][0]["oldText"], "old");
    assert_eq!(texts(&done[0]), ["Operation aborted"]);
}

#[test]
fn cancellation_waits_for_authoritative_tool_end_and_clears_queued_updates() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    start(&mut bridge, "c", "bash", json!({"command":"work"}));
    partial(&mut bridge, "c", "bash", "one");
    partial(&mut bridge, "c", "bash", "one two");
    let cancel = bridge.on_session(Inbound::Notification {
        method: "session/cancel".into(),
        params: json!({}),
    });
    assert_eq!(harness(&cancel[0])["type"], "abort");
    assert!(
        bridge.next_tick().is_some(),
        "abort request is not tool completion"
    );
    let failed = end(&mut bridge, "c", "bash", "Command aborted", true);
    assert_eq!(update(&failed[0])["status"], "failed");
    let ended = bridge.on_harness(
        &json!({"type":"agent_end","messages":[{"role":"assistant","stopReason":"aborted"}]}),
    );
    assert_eq!(
        response(&ended[0], 2).as_ref().unwrap()["stopReason"],
        "cancelled"
    );
    assert!(bridge.next_tick().is_none());
    assert!(bridge.tick().is_empty());
    assert!(partial(&mut bridge, "c", "bash", "late").is_empty());
}

#[test]
fn turn_end_flushes_latest_observation_before_the_reply_and_drops_its_timer() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    start(&mut bridge, "c", "bash", json!({"command":"work"}));
    partial(&mut bridge, "c", "bash", "one");
    partial(&mut bridge, "c", "bash", "latest");
    let outs = bridge.on_harness(&json!({"type":"agent_end","messages":[]}));
    assert_eq!(texts(&outs[0]), ["latest"]);
    assert_eq!(
        update(&outs[0])["status"],
        "in_progress",
        "no invented tool success"
    );
    assert!(response(&outs[1], 2).is_ok());
    assert!(bridge.tick().is_empty());
    assert!(bridge.next_tick().is_none());
    assert!(partial(&mut bridge, "c", "bash", "late").is_empty());
}

#[test]
fn excess_streams_keep_start_and_final_updates_without_growing_preview_storage() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let mut live = 0;
    let mut excess = 0;
    for index in 0..40 {
        let id = format!("c{index}");
        assert!(!start(&mut bridge, &id, "bash", json!({"command":"work"})).is_empty());
        if partial(&mut bridge, &id, "bash", "progress").is_empty() {
            excess += 1;
        } else {
            live += 1;
        }
    }
    assert!(live > 0 && excess > 0);
    for index in 0..40 {
        let done = end(&mut bridge, &format!("c{index}"), "bash", "final", false);
        assert_eq!(texts(&done[0]), ["final"]);
    }
    assert!(bridge.tick().is_empty());
}

#[test]
fn hostile_partial_content_has_finite_text_media_and_block_budgets() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    start(&mut bridge, "c", "custom", json!({}));
    let huge_text = "α".repeat(crate::project::MAX_ENTRY_TEXT);
    let huge_image = "A".repeat(1024 * 1024 + 4);
    let result = json!({"content":[{"type":"image","data":huge_image,"mimeType":"image/png"},{"type":"text","text":huge_text}]});
    let outs = bridge.on_harness(&json!({"type":"tool_execution_update","toolCallId":"c","toolName":"custom","partialResult":result}));
    let content = update(&outs[0])["content"].as_array().unwrap();
    assert!(
        content
            .iter()
            .all(|block| block.pointer("/content/data").is_none())
    );
    assert!(
        texts(&outs[0]).iter().map(|text| text.len()).sum::<usize>()
            <= crate::project::MAX_ENTRY_TEXT
    );
    assert!(
        texts(&outs[0])
            .iter()
            .any(|text| text.contains("truncated"))
    );
    let blocks = vec![json!({"type":"text","text":"tiny"}); 1000];
    bridge.on_harness(&json!({"type":"tool_execution_update","toolCallId":"c","toolName":"custom","partialResult":{"content":blocks}}));
    let outs = bridge.tick();
    assert!(update(&outs[0])["content"].as_array().unwrap().len() < 1000);
}

#[test]
fn exactly_full_text_budget_keeps_a_notice_for_later_omitted_content() {
    let limit = crate::project::MAX_ENTRY_TEXT * 3 / 4;
    for (later, marker) in [
        (
            vec![json!({"type":"text","text":"later text"})],
            "truncated",
        ),
        (
            vec![json!({"type":"image","data":"A".repeat(1024 * 1024 + 4),"mimeType":"image/png"})],
            "truncated",
        ),
        (
            vec![json!({"type":"text","text":"later text"}); 32],
            "Additional result blocks omitted",
        ),
    ] {
        let mut bridge = opened();
        prompt(&mut bridge, 2);
        start(&mut bridge, "c", "custom", json!({}));
        let mut content = vec![json!({"type":"text","text":"α".repeat(limit / 2)})];
        content.extend(later);
        let outs = bridge.on_harness(&json!({"type":"tool_execution_update","toolCallId":"c","toolName":"custom","partialResult":{"content":content}}));
        let text = texts(&outs[0]);
        assert!(text.iter().map(|value| value.len()).sum::<usize>() <= limit);
        assert!(text.last().unwrap().contains(marker));
        assert!(update(&outs[0])["content"].as_array().unwrap().len() <= 33);
    }
}

#[test]
fn partial_images_are_typed_and_final_images_use_the_larger_authoritative_budget() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    start(&mut bridge, "c", "custom", json!({}));
    let result = json!({"content":[{"type":"image","data":"AAAA","mimeType":"image/png"}]});
    let outs = bridge.on_harness(&json!({"type":"tool_execution_update","toolCallId":"c","toolName":"custom","partialResult":result}));
    assert_eq!(update(&outs[0])["content"][0]["content"]["data"], "AAAA");
    let data = "A".repeat(1024 * 1024 + 4);
    let done = bridge.on_harness(&json!({"type":"tool_execution_end","toolCallId":"c","toolName":"custom","isError":false,"result":{"content":[{"type":"image","data":data,"mimeType":"image/png"}]}}));
    assert_eq!(
        update(&done[0])["content"][0]["content"]["data"]
            .as_str()
            .unwrap()
            .len(),
        1024 * 1024 + 4
    );
}
