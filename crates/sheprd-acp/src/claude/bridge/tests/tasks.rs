//! Messages as Claude Code 2.1.318 sent them with its task tools on.

use super::*;

fn call(bridge: &mut Bridge, id: &str, name: &str, input: Value) -> Vec<Out> {
    bridge.on_harness(&json!({
        "type": "assistant",
        "parent_tool_use_id": null,
        "message": {"id": format!("msg_{id}"), "role": "assistant", "content": [
            {"type": "tool_use", "id": id, "name": name, "input": input},
        ]},
    }))
}

fn finished(bridge: &mut Bridge, id: &str, output: &str, structured: Value) -> Vec<Out> {
    bridge.on_harness(&json!({
        "type": "user",
        "parent_tool_use_id": null,
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": id, "content": output},
        ]},
        "tool_use_result": structured,
    }))
}

fn create(bridge: &mut Bridge, id: &str, subject: &str) -> Vec<Out> {
    assert!(
        call(
            bridge,
            &format!("toolu_{id}"),
            "TaskCreate",
            json!({"subject": subject, "description": subject}),
        )
        .is_empty(),
        "the call shows no card"
    );
    finished(
        bridge,
        &format!("toolu_{id}"),
        &format!("Task #{id} created successfully: {subject}"),
        json!({"task": {"id": id, "subject": subject, "description": subject}}),
    )
}

fn change(bridge: &mut Bridge, call_id: &str, task: &str, status: &str) -> Vec<Out> {
    assert!(
        call(
            bridge,
            call_id,
            "TaskUpdate",
            json!({"taskId": task, "status": status}),
        )
        .is_empty()
    );
    finished(
        bridge,
        call_id,
        &format!("Updated task #{task} status"),
        json!({"success": true, "taskId": task, "updatedFields": ["status"]}),
    )
}

/// The steps a lone plan update lists, as `(content, status)`.
fn steps(outs: &[Out]) -> Vec<(String, String)> {
    assert_eq!(outs.len(), 1, "one update");
    let plan = update(&outs[0]);
    assert_eq!(plan["sessionUpdate"], "plan");
    plan["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["content"].as_str().unwrap().to_owned(),
                entry["status"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(content, status)| ((*content).to_owned(), (*status).to_owned()))
        .collect()
}

fn started() -> Bridge {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "plan");
    bridge
}

#[test]
fn the_task_tools_build_the_plan() {
    let mut bridge = started();
    assert_eq!(
        steps(&create(&mut bridge, "1", "alpha")),
        pairs(&[("alpha", "pending")])
    );
    assert_eq!(
        steps(&create(&mut bridge, "2", "beta")),
        pairs(&[("alpha", "pending"), ("beta", "pending")])
    );
    assert_eq!(
        steps(&change(&mut bridge, "toolu_u1", "1", "in_progress")),
        pairs(&[("alpha", "in_progress"), ("beta", "pending")])
    );
    assert_eq!(
        steps(&change(&mut bridge, "toolu_u2", "1", "completed")),
        pairs(&[("alpha", "completed"), ("beta", "pending")])
    );
    assert_eq!(
        steps(&change(&mut bridge, "toolu_u3", "2", "deleted")),
        pairs(&[("alpha", "completed")])
    );
}

#[test]
fn the_list_outlives_the_turn() {
    let mut bridge = started();
    create(&mut bridge, "1", "alpha");
    bridge.on_harness(&result("success", Some("end_turn")));
    prompt(&mut bridge, 3, "go on");
    assert_eq!(
        steps(&change(&mut bridge, "toolu_u1", "1", "completed")),
        pairs(&[("alpha", "completed")])
    );
}

#[test]
fn claudes_own_list_replaces_this_one() {
    let mut bridge = started();
    create(&mut bridge, "1", "alpha");
    call(&mut bridge, "toolu_l", "TaskList", json!({}));
    let outs = finished(
        &mut bridge,
        "toolu_l",
        "#4 [in_progress] resumed\n#5 [completed] earlier",
        json!({"tasks": [
            {"id": "4", "subject": "resumed", "status": "in_progress", "blockedBy": []},
            {"id": "5", "subject": "earlier", "status": "completed", "blockedBy": []},
        ]}),
    );
    assert_eq!(
        steps(&outs),
        pairs(&[("resumed", "in_progress"), ("earlier", "completed")])
    );
}

#[test]
fn a_call_that_changes_nothing_sends_no_plan() {
    let mut bridge = started();
    create(&mut bridge, "1", "alpha");
    assert!(
        change(&mut bridge, "toolu_u1", "1", "pending").is_empty(),
        "same status"
    );
    call(&mut bridge, "toolu_g", "TaskGet", json!({"taskId": "1"}));
    assert!(finished(&mut bridge, "toolu_g", "Task #1: alpha", json!({})).is_empty());
    assert!(
        change(&mut bridge, "toolu_u2", "9", "completed").is_empty(),
        "a task with no subject to show"
    );
}

#[test]
fn a_refused_or_failed_call_leaves_the_plan() {
    let mut bridge = started();
    create(&mut bridge, "1", "alpha");
    call(
        &mut bridge,
        "toolu_u1",
        "TaskUpdate",
        json!({"taskId": "1", "status": "completed"}),
    );
    assert!(
        finished(
            &mut bridge,
            "toolu_u1",
            "Task #1 not found",
            json!({"success": false, "taskId": "1", "updatedFields": []}),
        )
        .is_empty()
    );
    call(
        &mut bridge,
        "toolu_u2",
        "TaskUpdate",
        json!({"taskId": "1", "status": "completed"}),
    );
    let failed = bridge.on_harness(&json!({
        "type": "user",
        "parent_tool_use_id": null,
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "toolu_u2", "content": "no", "is_error": true},
        ]},
    }));
    assert!(failed.is_empty());
}

#[test]
fn a_new_task_is_read_from_its_text_when_results_share_a_message() {
    let mut bridge = started();
    for (id, subject) in [("toolu_a", "alpha"), ("toolu_b", "beta")] {
        call(&mut bridge, id, "TaskCreate", json!({"subject": subject}));
    }
    let outs = bridge.on_harness(&json!({
        "type": "user",
        "parent_tool_use_id": null,
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "toolu_a",
             "content": "Task #1 created successfully: alpha"},
            {"type": "tool_result", "tool_use_id": "toolu_b",
             "content": [{"type": "text", "text": "Task #2 created successfully: beta"}]},
        ]},
    }));
    assert_eq!(outs.len(), 2);
    assert_eq!(
        steps(&outs[1..]),
        pairs(&[("alpha", "pending"), ("beta", "pending")])
    );
}

#[test]
fn a_sub_agents_task_calls_are_not_the_plan() {
    let mut bridge = started();
    let outs = bridge.on_harness(&json!({
        "type": "assistant",
        "parent_tool_use_id": "toolu_agent",
        "message": {"id": "msg_1", "role": "assistant", "content": [
            {"type": "tool_use", "id": "toolu_c", "name": "TaskCreate",
             "input": {"subject": "inner"}},
        ]},
    }));
    assert!(outs.is_empty());
    let outs = bridge.on_harness(&json!({
        "type": "user",
        "parent_tool_use_id": "toolu_agent",
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "toolu_c",
             "content": "Task #1 created successfully: inner"},
        ]},
    }));
    assert!(outs.is_empty());
}

#[test]
fn task_stop_is_still_a_call() {
    let mut bridge = started();
    let outs = call(
        &mut bridge,
        "toolu_s",
        "TaskStop",
        json!({"task_id": "shell"}),
    );
    assert_eq!(update(&outs[0])["sessionUpdate"], "tool_call");
}

/// The results of `calls` in one message, which leaves out the structured
/// form, each as `(call id, text)`.
fn together(bridge: &mut Bridge, calls: &[(&str, &str)]) -> Vec<Out> {
    let content: Vec<Value> = calls
        .iter()
        .map(|(id, text)| json!({"type": "tool_result", "tool_use_id": id, "content": text}))
        .collect();
    bridge.on_harness(&json!({
        "type": "user",
        "parent_tool_use_id": null,
        "message": {"role": "user", "content": content},
        "tool_use_result": {"tasks": []},
    }))
}

#[test]
fn a_list_is_read_from_its_text_when_results_share_a_message() {
    let mut bridge = started();
    create(&mut bridge, "1", "Fix it (urgent)");
    call(&mut bridge, "toolu_r", "Read", json!({"file_path": "/a"}));
    call(&mut bridge, "toolu_l", "TaskList", json!({}));
    let outs = together(
        &mut bridge,
        &[
            ("toolu_r", "text"),
            (
                "toolu_l",
                "#1 [in_progress] Fix it (urgent) (agent-1)\n\
                 #2 [pending] Ship it [blocked by #1, #3]\n\
                 #3 [completed] Earlier (agent-2)",
            ),
        ],
    );
    assert_eq!(outs.len(), 2, "the read's card and the plan");
    assert_eq!(
        steps(&outs[1..]),
        pairs(&[
            ("Fix it (urgent)", "in_progress"),
            ("Ship it", "pending"),
            ("Earlier (agent-2)", "completed"),
        ]),
        "a known subject stands; an owner stays on one not known"
    );
}

#[test]
fn an_empty_list_in_text_clears_the_plan() {
    let mut bridge = started();
    create(&mut bridge, "1", "alpha");
    call(&mut bridge, "toolu_r", "Read", json!({"file_path": "/a"}));
    call(&mut bridge, "toolu_l", "TaskList", json!({}));
    let outs = together(
        &mut bridge,
        &[("toolu_r", "text"), ("toolu_l", "No tasks found")],
    );
    assert_eq!(steps(&outs[1..]), pairs(&[]));
}

#[test]
fn text_that_is_not_all_a_list_leaves_the_plan() {
    let mut bridge = started();
    create(&mut bridge, "1", "alpha");
    for (id, text) in [
        ("toolu_a", "#1 [completed] alpha\nsomething else"),
        ("toolu_b", "#1 [odd] alpha"),
        ("toolu_c", "Could not read the tasks"),
    ] {
        let read = format!("{id}_read");
        call(&mut bridge, &read, "Read", json!({"file_path": "/a"}));
        call(&mut bridge, id, "TaskList", json!({}));
        let outs = together(&mut bridge, &[(&read, "text"), (id, text)]);
        assert_eq!(outs.len(), 1, "only the read's card for {id}");
    }
}
