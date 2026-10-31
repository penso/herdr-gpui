use super::*;

fn start(bridge: &mut Bridge, id: &str, name: &str) -> Vec<Out> {
    bridge.on_harness(&json!({
        "type": "assistant", "session_id": SESSION, "parent_tool_use_id": null,
        "message": {"id": "message", "content": [{
            "type": "tool_use", "id": id, "name": name,
            "input": {"command": "cargo test\n--quiet", "description": "Inspect tests"},
        }]},
    }))
}

fn tool_progress(id: &str, elapsed: Value) -> Value {
    json!({"type": "tool_progress", "session_id": SESSION, "parent_tool_use_id": null,
        "tool_use_id": id, "tool_name": "Bash", "elapsed_time_seconds": elapsed})
}

fn tool_result(id: &str, content: &str) -> Value {
    json!({"type": "user", "session_id": SESSION, "parent_tool_use_id": null,
        "message": {"content": [{"type": "tool_result", "tool_use_id": id, "content": content}]}})
}

fn task(kind: &str, id: &str) -> Value {
    json!({"type": "system", "subtype": kind, "session_id": SESSION,
        "task_id": id, "run_id": "run-001", "tool_use_id": "tool-1",
        "description": "Inspect tests", "summary": "Reading the fixtures",
        "usage": {"tool_uses": 2, "duration_ms": 3500, "total_tokens": 80},
        "last_tool_name": "Read"})
}

fn running() -> Bridge {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "inspect");
    start(&mut bridge, "tool-1", "Bash");
    bridge
}

#[test]
fn reported_tool_elapsed_preserves_input_and_final_output_supersedes_queued_progress() {
    let mut bridge = running();
    let first = bridge.on_harness(&tool_progress("tool-1", json!(1.2)));
    assert_eq!(update(&first[0])["status"], "in_progress");
    assert!(
        update(&first[0])["title"]
            .as_str()
            .unwrap()
            .ends_with("Running for 1s")
    );
    assert!(
        update(&first[0]).get("content").is_none(),
        "the command stays visible"
    );
    assert!(
        bridge
            .on_harness(&tool_progress("tool-1", json!(1.8)))
            .is_empty()
    );
    assert!(
        bridge
            .on_harness(&tool_progress("tool-1", json!(0)))
            .is_empty()
    );
    assert!(
        bridge
            .on_harness(&tool_progress("tool-1", json!(2)))
            .is_empty()
    );
    assert!(bridge.next_tick().is_some());
    let final_update = bridge.on_harness(&tool_result("tool-1", "all tests passed"));
    assert_eq!(update(&final_update[0])["status"], "completed");
    assert_eq!(
        update(&final_update[0])["title"],
        "`cargo test` (+1 more line)"
    );
    assert_eq!(
        update(&final_update[0])["content"][0]["content"]["text"],
        "all tests passed"
    );
    assert!(bridge.next_tick().is_none());
    assert!(bridge.tick().is_empty());
    assert!(
        bridge
            .on_harness(&tool_result("tool-1", "stale replacement"))
            .is_empty()
    );
    assert!(
        bridge
            .on_harness(&tool_progress("tool-1", json!(3)))
            .is_empty()
    );
    assert!(
        start(&mut bridge, "tool-1", "Bash").is_empty(),
        "a replay cannot reopen the call"
    );
}

#[test]
fn tool_progress_requires_current_root_call_and_never_changes_a_question() {
    let mut bridge = running();
    let valid = tool_progress("tool-1", json!(3));
    for (key, value) in [
        ("session_id", json!("another-session")),
        ("session_id", Value::Null),
        ("parent_tool_use_id", json!("nested")),
        ("tool_use_id", json!("unknown")),
        ("tool_name", json!("Read")),
        ("elapsed_time_seconds", json!(-1)),
        ("elapsed_time_seconds", json!("3")),
        ("elapsed_time_seconds", json!(1e100)),
    ] {
        let mut message = valid.clone();
        message[key] = value;
        assert!(bridge.on_harness(&message).is_empty(), "{message}");
    }
    for name in [tools::QUESTION, tools::EXIT_PLAN, tools::TODOS] {
        start(&mut bridge, name, name);
        let mut message = tool_progress(name, json!(1));
        message["tool_name"] = json!(name);
        assert!(bridge.on_harness(&message).is_empty());
        let mut background = task("task_started", name);
        background["tool_use_id"] = json!(name);
        assert!(bridge.on_harness(&background).is_empty());
    }
    let mut foreign = tool_result("tool-1", "must not complete this session");
    foreign["session_id"] = json!("another-session");
    assert!(bridge.on_harness(&foreign).is_empty());
    foreign["session_id"] = json!(42);
    assert!(bridge.on_harness(&foreign).is_empty());
    assert_eq!(
        bridge.on_harness(&valid).len(),
        1,
        "foreign finals leave the active call intact"
    );
    bridge.on_harness(&result("success", None));
    assert!(bridge.on_harness(&valid).is_empty(), "no active prompt");
}

#[test]
fn background_progress_survives_launch_result_and_turn_end_without_reading_output_file() {
    let mut bridge = running();
    let started = bridge.on_harness(&task("task_started", "task-1"));
    let row = update(&started[0])["toolCallId"].clone();
    assert_ne!(row, "tool-1");
    bridge.on_harness(&tool_result("tool-1", "Background task launched"));
    bridge.on_harness(&result("success", None));
    let progress = bridge.on_harness(&task("task_progress", "task-1"));
    let progress = update(&progress[0]);
    assert_eq!(progress["toolCallId"], row);
    let content = progress["content"][0]["content"]["text"].as_str().unwrap();
    assert!(content.contains("Reported progress: Reading the fixtures"));
    assert!(content.contains("Latest tool: Read"));
    assert!(content.contains("Tools used: 2\nElapsed: 3s\nTokens: 80"));
    let mut completed = task("task_notification", "task-1");
    completed["status"] = json!("completed");
    completed["summary"] = json!("Found the relevant fixture");
    completed["output_file"] = json!("/unread/synthetic/private-task-output");
    let done = bridge.on_harness(&completed);
    assert_eq!(update(&done[0])["toolCallId"], row);
    assert_eq!(update(&done[0])["status"], "completed");
    assert!(
        !serde_json::to_string(update(&done[0]))
            .unwrap()
            .contains("/unread/")
    );
    assert!(bridge.on_harness(&completed).is_empty());
    assert!(
        bridge
            .on_harness(&task("task_progress", "task-1"))
            .is_empty()
    );
    assert!(
        bridge
            .on_harness(&task("task_started", "task-1"))
            .is_empty()
    );
}

#[test]
fn task_terminal_patch_discards_queued_progress_and_allows_one_final_summary() {
    let mut bridge = running();
    bridge.on_harness(&task("task_started", "task-1"));
    bridge.on_harness(&task("task_progress", "task-1"));
    let mut progress = task("task_progress", "task-1");
    progress["summary"] = json!("Pending detail");
    assert!(bridge.on_harness(&progress).is_empty());
    assert!(bridge.next_tick().is_some());
    let mut stopped = task("task_updated", "task-1");
    stopped["patch"] = json!({"status": "killed"});
    let done = bridge.on_harness(&stopped);
    assert_eq!(update(&done[0])["status"], "failed");
    assert!(
        update(&done[0])["content"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .starts_with("Task stopped")
    );
    assert!(bridge.next_tick().is_none());
    assert!(bridge.tick().is_empty());
    assert!(bridge.on_harness(&stopped).is_empty());
    let mut notice = task("task_notification", "task-1");
    notice["status"] = json!("stopped");
    notice["summary"] = json!("Stopped by user");
    let final_update = bridge.on_harness(&notice);
    assert!(
        update(&final_update[0])["content"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("Stopped by user")
    );
    assert!(bridge.on_harness(&notice).is_empty());
}

#[test]
fn resumed_task_run_rejects_old_and_unscoped_events() {
    let mut bridge = running();
    bridge.on_harness(&task("task_started", "task-1"));
    let mut completed = task("task_notification", "task-1");
    completed["status"] = json!("completed");
    bridge.on_harness(&completed);
    let mut resumed = task("task_started", "task-1");
    resumed["run_id"] = json!("run-002");
    let new = bridge.on_harness(&resumed);
    assert!(
        update(&new[0])["toolCallId"]
            .as_str()
            .unwrap()
            .ends_with("run-002")
    );
    assert!(bridge.on_harness(&completed).is_empty());
    assert!(
        bridge
            .on_harness(&task("task_progress", "task-1"))
            .is_empty()
    );
    let mut progress = task("task_progress", "task-1");
    progress.as_object_mut().unwrap().remove("run_id");
    assert!(bridge.on_harness(&progress).is_empty());
    progress["run_id"] = json!("run-002");
    progress["tool_use_id"] = json!("another-tool");
    assert!(bridge.on_harness(&progress).is_empty());
    progress.as_object_mut().unwrap().remove("tool_use_id");
    assert_eq!(bridge.on_harness(&progress).len(), 1);
}

#[test]
fn task_rows_exclude_unrelated_nested_and_ambient_activity_and_bound_utf8() {
    let mut bridge = running();
    for (key, value) in [
        ("session_id", json!("other")),
        ("task_id", json!("")),
        ("task_id", json!("x".repeat(257))),
        ("task_id", json!("task\n1")),
        ("run_id", json!("")),
        ("run_id", Value::Null),
        ("tool_use_id", json!("unknown")),
        ("parent_task_id", json!("nested")),
        ("parent_tool_use_id", json!("nested")),
        ("spawn_depth", json!(2)),
        ("spawn_depth", json!("2")),
        ("spawn_depth", json!(-1)),
        ("spawn_depth", Value::Null),
        ("skip_transcript", json!(true)),
        ("skip_transcript", json!("true")),
        ("skip_transcript", Value::Null),
        ("ambient", json!(true)),
        ("ambient", json!("true")),
        ("ambient", Value::Null),
    ] {
        let mut message = task("task_started", "task-1");
        message[key] = value;
        assert!(bridge.on_harness(&message).is_empty(), "{key}");
    }
    let mut message = task("task_started", "task-1");
    message["description"] = json!("界".repeat(5000));
    let start = bridge.on_harness(&message);
    assert!(update(&start[0])["title"].as_str().unwrap().len() <= 4102);
    message["subtype"] = json!("task_progress");
    message["summary"] = json!("é".repeat(5000));
    let progress = bridge.on_harness(&message);
    let text = update(&progress[0])["content"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(text.len() <= 4096);
    assert!(text.ends_with('…'));
}

#[test]
fn progress_slots_and_completed_task_identity_memory_are_bounded() {
    let mut bridge = running();
    assert!(start(&mut bridge, &"i".repeat(257), "Bash").is_empty());
    assert!(start(&mut bridge, "oversized-name", &"n".repeat(257)).is_empty());
    for index in 0..16 {
        assert_eq!(
            bridge
                .on_harness(&task("task_started", &format!("task-{index}")))
                .len(),
            1
        );
    }
    assert!(
        bridge
            .on_harness(&task("task_started", "excess"))
            .is_empty()
    );
    for index in 0..2000 {
        let id = format!("task-{index}");
        if index >= 16 {
            assert_eq!(bridge.on_harness(&task("task_started", &id)).len(), 1);
        }
        let mut completed = task("task_notification", &id);
        completed["status"] = json!("completed");
        assert_eq!(bridge.on_harness(&completed).len(), 1);
    }
    assert!(
        bridge
            .on_harness(&task("task_started", "capacity"))
            .is_empty()
    );
    assert!(
        bridge
            .on_harness(&task("task_started", "task-0"))
            .is_empty()
    );
    assert!(bridge.next_tick().is_none());
}

#[test]
fn task_progress_and_background_metadata_share_the_same_completion_fence() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "delegate");
    start(&mut bridge, "tool-1", "Agent");
    let started = bridge.on_harness(&task("task_started", "task-1"));
    let row = update(&started[0])["toolCallId"].clone();
    let mut launched = tool_result("tool-1", "Async agent launched");
    launched["tool_use_result"] = json!({"isAsync": true, "agentId": "task-1"});
    bridge.on_harness(&launched);
    bridge.on_harness(&result("success", None));
    let mut done = task("task_notification", "task-1");
    done["status"] = json!("completed");
    let mut foreign = done.clone();
    foreign["run_id"] = json!("old-run");
    assert!(bridge.on_harness(&foreign).is_empty());
    foreign = done.clone();
    foreign["session_id"] = json!("other");
    assert!(bridge.on_harness(&foreign).is_empty());
    let outs = bridge.on_harness(&done);
    assert_eq!(outs.len(), 2);
    assert_eq!(update(&outs[0])["toolCallId"], row);
    assert_eq!(update(&outs[0])["status"], "completed");
    assert_eq!(update(&outs[1])["toolCallId"], "tool-1");
    assert_eq!(
        update(&outs[1])["_meta"]["herdr"]["activity"]["state"],
        "finished"
    );
    assert!(bridge.on_harness(&done).is_empty());
}
