use super::*;

fn notification(task: &str, call: Option<&str>, status: &str) -> Value {
    let call = call.map_or_else(String::new, |call| {
        format!("<tool-use-id>{call}</tool-use-id>")
    });
    json!({"type": "user", "session_id": SESSION, "parent_tool_use_id": null,
        "message": {"content": format!("<task-notification><task-id>{task}</task-id>{call}\n<status>{status}</status><summary>Final summary</summary></task-notification>")}})
}

#[test]
fn text_notification_finishes_progress_and_metadata_and_discards_queued_work() {
    for (status, expected, activity) in [
        ("completed", "completed", "finished"),
        ("failed", "failed", "failed"),
        ("killed", "failed", "killed"),
    ] {
        let mut bridge = opened();
        prompt(&mut bridge, 2, "delegate");
        start(&mut bridge, "tool-1", "Agent");
        let started = bridge.on_harness(&task("task_started", "task-1"));
        let row = update(&started[0])["toolCallId"].clone();
        bridge.on_harness(&task("task_progress", "task-1"));
        let mut queued = task("task_progress", "task-1");
        queued["summary"] = json!("Queued update");
        assert!(bridge.on_harness(&queued).is_empty());
        assert!(bridge.next_tick().is_some());
        let notice = notification("task-1", Some("tool-1"), status);
        let done = bridge.on_harness(&notice);
        assert_eq!(done.len(), 2);
        assert_eq!(update(&done[0])["toolCallId"], row);
        assert_eq!(update(&done[0])["status"], expected);
        assert!(
            update(&done[0])["content"][0]["content"]["text"]
                .as_str()
                .unwrap()
                .contains("Final summary")
        );
        assert_eq!(
            update(&done[1])["_meta"]["herdr"]["activity"]["state"],
            activity
        );
        assert!(bridge.next_tick().is_none());
        assert!(bridge.tick().is_empty());
        assert!(bridge.on_harness(&notice).is_empty());
        assert!(bridge.on_harness(&queued).is_empty());
    }
}

#[test]
fn text_notifications_release_slots_for_later_tasks() {
    let mut bridge = running();
    // More than the live stream cap can run sequentially without using it up.
    for index in 0..32 {
        let id = format!("task-{index}");
        let started = bridge.on_harness(&task("task_started", &id));
        assert_eq!(started.len(), 1);
        let done = bridge.on_harness(&notification(&id, None, "completed"));
        assert_eq!(done.len(), 1);
        assert_eq!(update(&done[0])["status"], "completed");
    }
    assert!(bridge.next_tick().is_none());
}

#[test]
fn unscoped_notifications_cannot_finish_foreign_calls_or_a_resumed_run() {
    let mut bridge = running();
    bridge.on_harness(&task("task_started", "task-1"));
    let mut foreign = notification("task-1", Some("tool-1"), "completed");
    foreign["session_id"] = json!("other");
    assert!(bridge.on_harness(&foreign).is_empty());
    assert!(
        bridge
            .on_harness(&notification("task-1", Some("other"), "completed"))
            .is_empty()
    );
    assert!(
        bridge
            .on_harness(&notification("task-1", None, "unknown"))
            .is_empty()
    );
    bridge.on_harness(&notification("task-1", None, "completed"));
    let mut resumed = task("task_started", "task-1");
    resumed["run_id"] = json!("run-002");
    let started = bridge.on_harness(&resumed);
    let row = update(&started[0])["toolCallId"].clone();
    assert!(
        bridge
            .on_harness(&notification("task-1", None, "completed"))
            .is_empty()
    );
    let mut done = task("task_notification", "task-1");
    done["run_id"] = json!("run-002");
    done["status"] = json!("completed");
    let finished = bridge.on_harness(&done);
    assert_eq!(update(&finished[0])["toolCallId"], row);
    assert_eq!(update(&finished[0])["status"], "completed");
}
