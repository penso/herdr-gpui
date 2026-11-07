use super::*;

const CHILD: &str = "thr_child";

fn tag(update: &Value) -> &Value {
    &update["_meta"]["herdr"]
}

fn spawn(status: &str, states: Value) -> Value {
    json!({
        "type": "collabAgentToolCall", "id": "call_spawn", "tool": "spawnAgent",
        "status": status, "senderThreadId": THREAD, "receiverThreadIds": [CHILD],
        "prompt": "Explore the parser\nand report back", "agentsStates": states,
    })
}

/// A bridge whose turn spawned a sub-agent that now runs.
fn spawned() -> Bridge {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    let started = notify(
        &mut bridge,
        "item/started",
        json!({"threadId": THREAD, "item": spawn("inProgress", json!({}))}),
    );
    let call = update(&started[0]);
    assert_eq!(call["title"], "Explore the parser");
    assert_eq!(tag(call)["activity"]["kind"], "subagent");
    assert_eq!(tag(call)["activity"]["state"], "running");
    let completed = notify(
        &mut bridge,
        "item/completed",
        json!({"threadId": THREAD, "item": spawn("completed",
            json!({CHILD: {"status": "running", "message": null}}))}),
    );
    let done = update(&completed[0]);
    assert_eq!(done["status"], "completed");
    assert!(done.get("_meta").is_none(), "the sub-agent still runs");
    bridge
}

#[test]
fn a_sub_agents_calls_show_under_its_spawn_and_its_turn_ends_it() {
    let mut bridge = spawned();
    let started = notify(
        &mut bridge,
        "item/started",
        json!({"threadId": CHILD, "item": {"type": "commandExecution", "id": "exec-c",
            "command": "rg parse", "status": "inProgress"}}),
    );
    let step = update(&started[0]);
    assert_eq!(step["toolCallId"], "exec-c");
    assert_eq!(tag(step)["parent"], "call_spawn");
    let prose = notify(
        &mut bridge,
        "item/agentMessage/delta",
        json!({"threadId": CHILD, "itemId": "m", "delta": "inner"}),
    );
    assert!(prose.is_empty(), "its prose stays inside it");
    let child_end = notify(
        &mut bridge,
        "turn/completed",
        json!({"threadId": CHILD, "turn": {"id": "t-c", "status": "completed"}}),
    );
    assert_eq!(
        child_end.len(),
        1,
        "the session's own prompt is not answered"
    );
    let ended = update(&child_end[0]);
    assert_eq!(ended["toolCallId"], "call_spawn");
    assert_eq!(tag(ended)["activity"]["state"], "finished");
    let after = notify(
        &mut bridge,
        "item/started",
        json!({"threadId": CHILD, "item": {"type": "commandExecution", "id": "exec-d",
            "command": "ls", "status": "inProgress"}}),
    );
    assert!(
        after.is_empty(),
        "a finished sub-agent's thread is forgotten"
    );
}

#[test]
fn a_wait_that_reports_an_error_fails_the_sub_agent() {
    let mut bridge = spawned();
    let outs = notify(
        &mut bridge,
        "item/completed",
        json!({"threadId": THREAD, "item": {"type": "collabAgentToolCall", "id": "call_wait",
            "tool": "wait", "status": "completed", "senderThreadId": THREAD,
            "receiverThreadIds": [CHILD], "prompt": null,
            "agentsStates": {CHILD: {"status": "errored", "message": "Out of context"}}}}),
    );
    assert_eq!(update(&outs[0])["title"], "Wait for sub-agents");
    let failed = update(&outs[1]);
    assert_eq!(failed["toolCallId"], "call_spawn");
    assert_eq!(tag(failed)["activity"]["state"], "failed");
    assert_eq!(tag(failed)["activity"]["detail"], "Out of context");
}

#[test]
fn a_sub_agent_activity_item_ends_it_without_a_card() {
    let mut bridge = spawned();
    let outs = notify(
        &mut bridge,
        "item/completed",
        json!({"threadId": THREAD, "item": {"type": "subAgentActivity",
            "id": "subagent-completed-1", "kind": "completed", "agentThreadId": CHILD,
            "agentPath": "/root/explorer"}}),
    );
    assert_eq!(outs.len(), 1);
    let ended = update(&outs[0]);
    assert_eq!(ended["toolCallId"], "call_spawn");
    assert_eq!(tag(ended)["activity"]["state"], "finished");
}

#[test]
fn a_command_still_running_when_its_turn_ends_is_a_background_shell() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    for (id, command) in [("exec-bg", "npm run dev"), ("exec-quick", "ls")] {
        notify(
            &mut bridge,
            "item/started",
            json!({"threadId": THREAD, "item": {"type": "commandExecution", "id": id,
                "command": command, "status": "inProgress", "processId": "41"}}),
        );
    }
    notify(
        &mut bridge,
        "item/completed",
        json!({"threadId": THREAD, "item": {"type": "commandExecution", "id": "exec-quick",
            "command": "ls", "status": "completed", "exitCode": 0, "aggregatedOutput": "a"}}),
    );
    let outs = notify(
        &mut bridge,
        "turn/completed",
        json!({"threadId": THREAD, "turn": {"id": "turn-1", "status": "completed"}}),
    );
    assert_eq!(outs.len(), 2);
    let started = update(&outs[0]);
    assert_eq!(started["toolCallId"], "exec-bg");
    assert_eq!(tag(started)["activity"]["kind"], "shell");
    assert_eq!(tag(started)["activity"]["label"], "npm run dev");
    assert!(response(&outs[1], 2).is_ok());

    let outs = notify(
        &mut bridge,
        "item/completed",
        json!({"threadId": THREAD, "item": {"type": "commandExecution", "id": "exec-bg",
            "command": "npm run dev", "status": "failed", "exitCode": 130,
            "aggregatedOutput": "^C"}}),
    );
    let ended = update(&outs[0]);
    assert_eq!(ended["status"], "failed");
    assert_eq!(tag(ended)["activity"]["state"], "failed");
    assert_eq!(tag(ended)["activity"]["detail"], "Exited 130");
}

#[test]
fn a_failed_spawn_fails_its_sub_agent_and_unknown_threads_stay_skipped() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    notify(
        &mut bridge,
        "item/started",
        json!({"threadId": THREAD, "item": spawn("inProgress", json!({}))}),
    );
    let outs = notify(
        &mut bridge,
        "item/completed",
        json!({"threadId": THREAD, "item": spawn("failed", json!({}))}),
    );
    assert_eq!(tag(update(&outs[0]))["activity"]["state"], "failed");
    let child = notify(
        &mut bridge,
        "item/started",
        json!({"threadId": CHILD, "item": {"type": "commandExecution", "id": "x",
            "command": "ls", "status": "inProgress"}}),
    );
    assert!(child.is_empty(), "a failed spawn has no thread to follow");
}
