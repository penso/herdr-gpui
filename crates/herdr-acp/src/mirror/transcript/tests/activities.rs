use herdr_chat_protocol::{ActivityKind, ActivityState};

use super::*;

fn record(uuid: &str, time: &str, kind: &str, content: Value) -> Value {
    json!({
        "type": kind, "uuid": uuid, "sessionId": "s1", "cwd": "/work",
        "timestamp": time, "message": {"role": kind, "content": content},
    })
}

#[test]
fn a_background_shell_runs_from_its_call_to_its_notification() {
    let mut result = record(
        "u2",
        "2026-10-29T10:00:01.000Z",
        "user",
        json!([{"type": "tool_result", "tool_use_id": "toolu_bg",
                "content": "Command running in background with ID: b1."}]),
    );
    result["toolUseResult"] = json!({"backgroundTaskId": "b1"});
    let mut notification = record(
        "u3",
        "2026-10-29T10:02:00.500Z",
        "user",
        json!(
            "<task-notification>\n<task-id>b1</task-id>\n<status>completed</status>\n\
               <summary>Server stopped</summary>\n</task-notification>"
        ),
    );
    notification["origin"] = json!({"kind": "task-notification"});
    let running = lines(&[
        record(
            "u1",
            "2026-10-29T10:00:00.000Z",
            "assistant",
            json!([{"type": "tool_use", "id": "toolu_bg", "name": "Bash",
                    "input": {"command": "npm run dev", "run_in_background": true}}]),
        ),
        result,
    ]);
    let transcript = parse(TranscriptAgent::Claude, &running, 0);
    let activity = transcript.activity("toolu_bg").unwrap();
    assert_eq!(activity.kind, ActivityKind::Shell);
    assert_eq!(activity.label, "npm run dev");
    assert_eq!(activity.started_ms, 1_793_268_000_000);
    assert!(activity.is_running());

    let mut ended = running.clone();
    ended.extend(lines(&[notification]));
    let transcript = parse(TranscriptAgent::Claude, &ended, 0);
    let activity = transcript.activity("toolu_bg").unwrap();
    assert_eq!(activity.state, ActivityState::Finished);
    assert_eq!(activity.finished_ms, Some(1_793_268_120_500));
    assert_eq!(activity.detail.as_deref(), Some("Server stopped"));
}

#[test]
fn a_sub_agent_call_is_marked_and_ends_with_its_result() {
    let bytes = lines(&[
        record(
            "u1",
            "2026-10-29T10:00:00Z",
            "assistant",
            json!([{"type": "tool_use", "id": "toolu_a", "name": "Agent",
                    "input": {"description": "Explore"}}]),
        ),
        record(
            "u2",
            "2026-10-29T10:00:30Z",
            "user",
            json!([{"type": "tool_result", "tool_use_id": "toolu_a", "content": "done"}]),
        ),
    ]);
    let transcript = parse(TranscriptAgent::Claude, &bytes, 0);
    let EntryKind::Tool(tool) = &transcript.entries[0].kind else {
        panic!("tool")
    };
    assert_eq!(tool.kind, ToolKind::Subagent);
    let activity = transcript.activity("toolu_a").unwrap();
    assert_eq!(activity.state, ActivityState::Finished);
    assert_eq!(activity.finished_ms, Some(activity.started_ms + 30_000));
}

#[test]
fn records_without_a_time_or_a_known_origin_start_and_end_nothing() {
    let mut untimed = record(
        "u1",
        "",
        "assistant",
        json!([{"type": "tool_use", "id": "toolu_bg", "name": "Bash",
                "input": {"command": "sleep 9", "run_in_background": true}}]),
    );
    untimed["timestamp"] = json!("yesterday");
    let typed = record(
        "u2",
        "2026-10-29T10:00:00Z",
        "user",
        json!(
            "<task-notification><task-id>b1</task-id><status>completed</status></task-notification>"
        ),
    );
    let transcript = parse(TranscriptAgent::Claude, &lines(&[untimed, typed]), 0);
    assert!(transcript.activities.is_empty());
}
