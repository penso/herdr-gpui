//! Conforming ACP fixtures exercise the shared path, including an unknown
//! provider. These are protocol tests, not live-provider capability claims.

use super::*;
use herdr_chat_protocol::{Change, Tool, ToolContent, ToolStatus};

fn ready_for(agent: &str) -> Harness {
    let mut harness = harness_with(Config {
        agent: agent.into(),
        ..config()
    });
    harness.session.start();
    let id = harness.last_request("initialize")["id"].as_i64().unwrap();
    harness.respond(id, json!({"protocolVersion":1,"agentCapabilities":{}}));
    let id = harness.last_request("session/new")["id"].as_i64().unwrap();
    harness.respond(id, json!({"sessionId":"s1"}));
    harness
}

fn start(harness: &mut Harness) {
    harness.update(json!({"sessionUpdate":"tool_call", "toolCallId":"command",
        "title":"Run check", "kind":"execute", "status":"pending"}));
}

fn progress(text: &str) -> Value {
    json!({"sessionUpdate":"tool_call_update", "toolCallId":"command", "status":"in_progress",
        "content":[{"type":"content", "content":{"type":"text", "text":text}}]})
}

fn command(harness: &Harness) -> &Tool {
    harness
        .transcript()
        .entries
        .iter()
        .find_map(|entry| match &entry.kind {
            EntryKind::Tool(tool) if tool.call_id == "command" => Some(tool),
            _ => None,
        })
        .unwrap()
}

#[test]
fn any_acp_provider_streams_text_and_tool_content_before_the_turn_finishes() {
    for provider in [
        "Codex",
        "Claude",
        "Pi",
        "Copilot",
        "Gemini",
        "OpenCode",
        "Custom ACP",
    ] {
        let mut harness = ready_for(provider);
        let updates = harness.subscribe(1, false, None);
        let turn = prompt(&mut harness, 0, "check");
        drain(&updates);
        harness.agent_says("Checking ");
        harness.agent_says("now.");
        assert!(harness.transcript().entries.iter().any(
            |entry| matches!(&entry.kind, EntryKind::Agent { text } if text == "Checking now.")
        ));
        start(&mut harness);
        for text in ["first line", "first line\nsecond line"] {
            harness.update(progress(text));
            assert_eq!(harness.transcript().status, Status::Working, "{provider}");
            assert_eq!(command(&harness).status, ToolStatus::InProgress);
            assert_eq!(
                command(&harness).content,
                [ToolContent::Text { text: text.into() }]
            );
            assert!(drain(&updates).iter().any(|update| matches!(update,
                ServerMessage::Change { change: Change::Replace { entry }, .. }
                    if matches!(&entry.kind,EntryKind::Tool(tool) if tool.status == ToolStatus::InProgress))));
        }
        harness.update(
            json!({"sessionUpdate":"tool_call_update","toolCallId":"command",
            "status":"completed", "content":[]}),
        );
        assert_eq!(command(&harness).status, ToolStatus::Completed);
        assert!(
            command(&harness).content.is_empty(),
            "an empty final is authoritative"
        );
        assert_eq!(harness.transcript().status, Status::Working);
        harness.respond(turn, json!({"stopReason":"end_turn"}));
        assert_eq!(harness.transcript().status, Status::Idle);
    }
}

#[test]
fn live_tool_output_never_answers_a_permission_or_unlocks_a_blocked_session() {
    let mut harness = ready_for("Custom ACP");
    prompt(&mut harness, 0, "check");
    start(&mut harness);
    let permission = ask_permission(&mut harness, 91);
    harness.update(progress("work reported while a different permission waits"));
    assert_eq!(harness.transcript().status, Status::Working);
    assert_eq!(harness.reports.borrow().last(), Some(&Activity::Blocked));
    assert_eq!(harness.transcript().permissions.len(), 1);
    assert_eq!(harness.transcript().permissions[0].id, permission);
    assert_eq!(command(&harness).status, ToolStatus::InProgress);
    assert!(
        harness
            .sent()
            .iter()
            .all(|message| message.get("result").is_none())
    );
}

#[test]
fn a_tool_update_from_another_session_cannot_replace_this_sessions_output() {
    let mut harness = ready_for("Custom ACP");
    start(&mut harness);
    harness.update(progress("ours"));
    let before = harness.transcript().clone();
    harness.input(Input::Agent(Inbound::Notification {
        method: "session/update".into(),
        params: json!({"sessionId":"another-session","update":progress("theirs")}),
    }));
    assert_eq!(harness.transcript(), &before);
}
