#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use serde_json::json;
use sheprd_chat_protocol::ToolKind;

#[path = "tool_generations.rs"]
mod generations;

fn call(id: &str, name: &str, input: Value) -> Value {
    json!({"type":"response_item","payload":{
        "type":"function_call","call_id":id,"name":name,
        "arguments":input.to_string()
    }})
}

fn result(id: &str, output: Value) -> Value {
    json!({"type":"response_item","payload":{
        "type":"function_call_output","call_id":id,"output":output
    }})
}

fn project(agent: TranscriptAgent, records: &[Value]) -> Transcript {
    let bytes = records
        .iter()
        .map(|record| format!("{record}\n"))
        .collect::<String>();
    parse(agent, bytes.as_bytes(), 0)
}

fn tool(transcript: &Transcript, index: usize) -> &Tool {
    let EntryKind::Tool(tool) = &transcript.entries[index].kind else {
        panic!("tool")
    };
    tool
}

#[test]
fn claude_projects_tools_and_explicit_failures_without_approval_replay() {
    let input = json!({"command":"cargo check"});
    let transcript = project(
        TranscriptAgent::Claude,
        &[
            json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"c","name":"Bash","input":input}]}}),
            json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"c","is_error":true,"content":[{"type":"text","text":"Denied"}]}]}}),
        ],
    );
    let tool = tool(&transcript, 0);
    assert_eq!(
        (tool.kind, tool.title.as_str(), tool.status),
        (ToolKind::Execute, "`cargo check`", ToolStatus::Failed)
    );
    assert!(transcript.permissions.is_empty());
    assert!(transcript.read_only);
}

#[test]
fn command_results_distinguish_failure_running_and_returned_output() {
    for (output, expected) in [
        (json!({"exit_code":2,"output":"failed"}), ToolStatus::Failed),
        (
            json!(r#"{"exit_code":0,"output":"the word error does not mean failure"}"#),
            ToolStatus::Completed,
        ),
        (
            json!({"session_id":42,"exit_code":null,"output":"working"}),
            ToolStatus::InProgress,
        ),
        (
            json!(
                "Chunk ID: abcd\nWall time: 0.1 seconds\nProcess exited with code 17\nFinal output:\ntext"
            ),
            ToolStatus::Failed,
        ),
        (
            json!(
                "Chunk ID: abcd\nWall time: 0.1 seconds\nProcess running with session ID 42\nFinal output:\ntext"
            ),
            ToolStatus::InProgress,
        ),
        (
            json!("Wall time: 0.1 seconds\nExit code: 0\nOutput:\nProcess exited with code 7"),
            ToolStatus::Completed,
        ),
        (
            json!("Quoted output:\nProcess exited with code 7\nFinal output:\ntext"),
            ToolStatus::Completed,
        ),
        (json!("error: example documentation"), ToolStatus::Completed),
    ] {
        let transcript = project(
            TranscriptAgent::Codex,
            &[
                call("c", "exec_command", json!({"cmd":"test-command"})),
                result("c", output.clone()),
            ],
        );
        let tool = tool(&transcript, 0);
        assert_eq!(tool.status, expected, "{output}");
        let raw = output
            .as_str()
            .map_or_else(|| output.to_string(), str::to_owned);
        assert_eq!(tool.content.last(), Some(&ToolContent::Text { text: raw }));
    }
}

#[test]
fn exact_command_poll_completion_updates_only_its_running_history() {
    let records = [
        call("a", "exec_command", json!({"cmd":"job-a"})),
        result("a", json!({"session_id":41,"output":"a"})),
        call("b", "exec_command", json!({"cmd":"job-b"})),
        result("b", json!({"session_id":42,"output":"b"})),
        call("p1", "write_stdin", json!({"session_id":41})),
        result("p1", json!({"session_id":41,"output":"still working"})),
        call("p2", "write_stdin", json!({"session_id":41})),
        result("p2", json!({"exit_code":3,"output":"done"})),
    ];
    let transcript = project(TranscriptAgent::Codex, &records);
    assert_eq!(tool(&transcript, 0).status, ToolStatus::Failed);
    assert_eq!(tool(&transcript, 1).status, ToolStatus::InProgress);
    assert_eq!(tool(&transcript, 2).status, ToolStatus::Failed);
    assert_eq!(tool(&transcript, 3).status, ToolStatus::Failed);
    assert_eq!(
        tool(&transcript, 0).content.last(),
        Some(&ToolContent::Text {
            text: json!({"session_id":41,"output":"a"}).to_string()
        })
    );
}

#[test]
fn failed_poll_and_unknown_tool_do_not_finish_a_background_job() {
    let transcript = project(
        TranscriptAgent::Codex,
        &[
            call("c", "exec_command", json!({"cmd":"job"})),
            result("c", json!({"session_id":42,"output":"running"})),
            call("poll", "write_stdin", json!({"session_id":42})),
            result("poll", json!({"isError":true,"content":[]})),
            call(
                "unknown",
                "mcp__server__exec_command",
                json!({"session_id":42}),
            ),
            result("unknown", json!({"exit_code":1,"output":"business data"})),
        ],
    );
    assert_eq!(tool(&transcript, 0).status, ToolStatus::InProgress);
    assert_eq!(tool(&transcript, 1).status, ToolStatus::Failed);
    assert_eq!(tool(&transcript, 2).status, ToolStatus::Completed);
    assert_eq!(tool(&transcript, 2).kind, ToolKind::Other);
    assert_eq!(tool(&transcript, 2).title, "mcp__server__exec_command");
}

#[test]
fn opaque_scripts_stay_opaque_and_cell_waits_correlate_exactly() {
    let script = "await tools.exec_command({cmd: 'secret action'})";
    let transcript = project(
        TranscriptAgent::Codex,
        &[
            json!({"type":"response_item","payload":{"type":"custom_tool_call","call_id":"c","name":"functions.exec","input":script}}),
            result(
                "c",
                json!("Script running with cell ID 42\nWall time: 0.1 seconds\nOutput:\n"),
            ),
            call("shell", "exec_command", json!({"cmd":"job"})),
            result("shell", json!({"session_id":42,"output":"running"})),
            call("wait", "functions.wait", json!({"cell_id":"42"})),
            result(
                "wait",
                json!(
                    "Script completed\nWall time: 0.1 seconds\nOutput:\nProcess exited with code 1"
                ),
            ),
        ],
    );
    assert_eq!(tool(&transcript, 0).status, ToolStatus::Completed);
    assert_eq!(
        tool(&transcript, 0).content.first(),
        Some(&ToolContent::Text {
            text: script.into()
        })
    );
    assert_eq!(
        tool(&transcript, 1).status,
        ToolStatus::InProgress,
        "command IDs and cell IDs are separate namespaces"
    );
    assert_eq!(
        tool(&transcript, 2).status,
        ToolStatus::Completed,
        "wrapper completion is not inner-command success"
    );
}

#[test]
fn returning_from_agent_tools_does_not_claim_child_task_completion() {
    let transcript = project(
        TranscriptAgent::Codex,
        &[
            call(
                "spawn",
                "collaboration.spawn_agent",
                json!({"task_name":"review"}),
            ),
            result("spawn", json!({"agent_id":"agent-1","status":"running"})),
            call("wait", "collaboration.wait_agent", json!({})),
            result("wait", json!({"status":"timeout"})),
        ],
    );
    assert!(transcript.entries.iter().all(
        |entry| matches!(&entry.kind,EntryKind::Tool(tool) if tool.status == ToolStatus::Completed)
    ));
}

#[test]
fn script_failure_is_a_wrapper_status_not_a_word_in_its_output() {
    for (output, expected) in [
        (
            "Script failed\nWall time: 0.0 seconds\nOutput:\nScript error: failed",
            ToolStatus::Failed,
        ),
        (
            "Script completed\nWall time: 0.0 seconds\nOutput:\nScript failed",
            ToolStatus::Completed,
        ),
        (
            "Script failed\nWall time: not-a-number seconds\nOutput:\nquoted example",
            ToolStatus::Completed,
        ),
        ("Script error: some printed text", ToolStatus::Completed),
    ] {
        let transcript = project(
            TranscriptAgent::Codex,
            &[
                call("c", "functions.exec", json!({})),
                result("c", json!(output)),
            ],
        );
        assert_eq!(tool(&transcript, 0).status, expected, "{output}");
    }
}

#[test]
fn malformed_or_oversized_arguments_fall_back_without_execution() {
    for raw in [
        "{not json".to_owned(),
        "{\"cmd\":\"".to_owned() + &"x".repeat(TEXT) + "\"}",
    ] {
        let transcript = project(
            TranscriptAgent::Codex,
            &[
                json!({"type":"response_item","payload":{"type":"function_call","call_id":"c","name":"exec_command","arguments":raw}}),
            ],
        );
        let tool = tool(&transcript, 0);
        assert_eq!(tool.title, "exec_command");
        assert_eq!(tool.status, ToolStatus::Pending);
        assert!(tool.locations.is_empty());
        assert!(matches!(tool.content.first(),Some(ToolContent::Text{text}) if text.len() <= TEXT));
    }
}

#[test]
fn cut_tool_results_mark_truncation_and_keep_reported_failure() {
    let long = "α".repeat(TEXT);
    let wrapper = format!("Wall time: 0.1 seconds\nProcess exited with code 1\nOutput:\n{long}");
    for output in [
        json!({"exit_code":1,"output":long}),
        json!(wrapper),
        json!([{"type":"text","text":wrapper}]),
    ] {
        let transcript = project(
            TranscriptAgent::Codex,
            &[
                call("c", "exec_command", json!({"cmd":"example"})),
                result("c", output),
            ],
        );
        assert!(transcript.truncated);
        let tool = tool(&transcript, 0);
        assert_eq!(tool.status, ToolStatus::Failed);
        assert!(
            matches!(tool.content.last(), Some(ToolContent::Text { text }) if text.len() <= TEXT)
        );
    }
}
