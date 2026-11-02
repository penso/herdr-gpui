use super::*;

fn assistant(id: &str, parent: Option<&str>, content: Value) -> Value {
    json!({
        "type": "assistant",
        "message": {"id": id, "role": "assistant", "content": content},
        "parent_tool_use_id": parent,
        "session_id": SESSION,
    })
}

fn tool_result(id: &str, parent: Option<&str>, content: Value, structured: Value) -> Value {
    json!({
        "type": "user",
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": id, "content": content, "is_error": false},
        ]},
        "parent_tool_use_id": parent,
        "tool_use_result": structured,
        "session_id": SESSION,
    })
}

fn tag(update: &Value) -> &Value {
    &update["_meta"]["herdr"]
}

#[test]
fn a_subagents_calls_show_under_it_and_its_prose_does_not() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "delegate");
    let outs = bridge.on_harness(&assistant(
        "msg_1",
        None,
        json!([{"type": "tool_use", "id": "toolu_task", "name": "Agent",
                "input": {"description": "Explore the parser", "prompt": "Look around"}}]),
    ));
    let call = update(&outs[0]);
    assert_eq!(call["title"], "Explore the parser");
    assert_eq!(tag(call)["activity"]["kind"], "subagent");
    assert_eq!(tag(call)["activity"]["state"], "running");

    let outs = bridge.on_harness(&assistant(
        "msg_2",
        Some("toolu_task"),
        json!([
            {"type": "text", "text": "Let me look."},
            {"type": "tool_use", "id": "toolu_read", "name": "Read",
             "input": {"file_path": "/work/parser.rs"}},
            {"type": "tool_use", "id": "toolu_todo", "name": "TodoWrite",
             "input": {"todos": []}},
        ]),
    ));
    assert_eq!(outs.len(), 1, "only the call; no prose, no plan");
    let step = update(&outs[0]);
    assert_eq!(step["toolCallId"], "toolu_read");
    assert_eq!(tag(step)["parent"], "toolu_task");

    let outs = bridge.on_harness(&tool_result(
        "toolu_read",
        Some("toolu_task"),
        json!("fn parse()"),
        Value::Null,
    ));
    assert_eq!(update(&outs[0])["status"], "completed");

    let outs = bridge.on_harness(&tool_result(
        "toolu_task",
        None,
        json!([{"type": "text", "text": "The parser is fine."}]),
        json!({"status": "completed", "agentId": "a1"}),
    ));
    let done = update(&outs[0]);
    assert_eq!(done["status"], "completed");
    assert_eq!(tag(done)["activity"]["state"], "finished");
}

#[test]
fn a_background_shell_outlives_its_turn_until_claude_says_it_ended() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "serve");
    let outs = bridge.on_harness(&assistant(
        "msg_1",
        None,
        json!([{"type": "tool_use", "id": "toolu_bg", "name": "Bash",
                "input": {"command": "npm run dev", "run_in_background": true}}]),
    ));
    let call = update(&outs[0]);
    assert_eq!(tag(call)["activity"]["kind"], "shell");
    assert_eq!(tag(call)["activity"]["label"], "npm run dev");
    let outs = bridge.on_harness(&tool_result(
        "toolu_bg",
        None,
        json!(
            "Command running in background with ID: ba2w31ujt. Output is being written to: /tmp/x"
        ),
        json!({"stdout": "", "stderr": "", "interrupted": false, "backgroundTaskId": "ba2w31ujt"}),
    ));
    let started = update(&outs[0]);
    assert_eq!(started["status"], "completed", "the call itself is done");
    assert!(started.get("_meta").is_none(), "the shell still runs");

    let outs = bridge.on_harness(&result("success", Some("end_turn")));
    assert!(
        outs.iter().all(
            |out| !matches!(out, Out::Session(Inbound::Notification { .. }))
                || update(out).get("_meta").is_none()
        ),
        "the turn's end does not stop it"
    );

    let outs = bridge.on_harness(&json!({
        "type": "system", "subtype": "task_notification", "task_id": "ba2w31ujt",
        "tool_use_id": "toolu_bg", "status": "completed",
        "output_file": "/tmp/x", "summary": "Dev server exited", "session_id": SESSION,
    }));
    let ended = update(&outs[0]);
    assert_eq!(ended["sessionUpdate"], "tool_call_update");
    assert_eq!(ended["toolCallId"], "toolu_bg");
    assert_eq!(tag(ended)["activity"]["state"], "finished");
    assert_eq!(tag(ended)["activity"]["detail"], "Dev server exited");
    assert!(ended.get("status").is_none(), "its card is left as it was");
}

#[test]
fn a_live_task_notification_prompt_ends_the_shell_it_names() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "serve");
    bridge.on_harness(&assistant(
        "msg_1",
        None,
        json!([{"type": "tool_use", "id": "toolu_bg", "name": "Bash",
                "input": {"command": "sleep 60", "run_in_background": true}}]),
    ));
    bridge.on_harness(&tool_result(
        "toolu_bg",
        None,
        json!("Command running in background with ID: b77."),
        Value::Null,
    ));
    let outs = bridge.on_harness(&json!({
        "type": "user",
        "message": {"role": "user", "content":
            "<task-notification>\n<task-id>b77</task-id>\n<status>killed</status>\n\
             <summary>Stopped</summary>\n</task-notification>"},
        "parent_tool_use_id": null,
        "session_id": SESSION,
    }));
    let ended = update(&outs[0]);
    assert_eq!(ended["toolCallId"], "toolu_bg");
    assert_eq!(tag(ended)["activity"]["state"], "killed");
}

#[test]
fn task_stop_reports_the_shell_it_killed() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "serve");
    bridge.on_harness(&assistant(
        "msg_1",
        None,
        json!([{"type": "tool_use", "id": "toolu_bg", "name": "Bash",
                "input": {"command": "sleep 60", "run_in_background": true}}]),
    ));
    bridge.on_harness(&tool_result(
        "toolu_bg",
        None,
        json!(""),
        json!({"backgroundTaskId": "b9"}),
    ));
    bridge.on_harness(&assistant(
        "msg_2",
        None,
        json!([{"type": "tool_use", "id": "toolu_stop", "name": "TaskStop",
                "input": {"task_id": "b9"}}]),
    ));
    let outs = bridge.on_harness(&tool_result(
        "toolu_stop",
        None,
        json!("Stopped b9"),
        json!({"message": "Stopped", "task_id": "b9", "task_type": "local_bash"}),
    ));
    assert_eq!(outs.len(), 2);
    assert_eq!(update(&outs[0])["toolCallId"], "toolu_stop");
    let killed = update(&outs[1]);
    assert_eq!(killed["toolCallId"], "toolu_bg");
    assert_eq!(tag(killed)["activity"]["state"], "killed");
}

#[test]
fn an_interrupted_turn_stops_its_foreground_subagent() {
    let mut bridge = opened();
    prompt(&mut bridge, 2, "delegate");
    bridge.on_harness(&assistant(
        "msg_1",
        None,
        json!([{"type": "tool_use", "id": "toolu_task", "name": "Agent",
                "input": {"description": "Explore"}}]),
    ));
    let outs = bridge.on_harness(&result("error_during_execution", None));
    let stopped = outs
        .iter()
        .filter(|out| matches!(out, Out::Session(Inbound::Notification { .. })))
        .map(update)
        .find(|update| update["toolCallId"] == "toolu_task")
        .unwrap();
    assert_eq!(tag(stopped)["activity"]["state"], "killed");
}
