#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{Value, json};

use super::*;

mod activities;
mod images;

fn update(value: Value) -> wire::SessionUpdate {
    serde_json::from_value(value).unwrap()
}

fn apply(projector: &mut Projector, transcript: &mut Transcript, value: Value) {
    for change in projector.update(transcript, update(value), 1_000) {
        transcript.apply(&change).unwrap();
    }
}

fn chunk(kind: &str, text: &str, message: Option<&str>) -> Value {
    let mut value = json!({"sessionUpdate": kind, "content": {"type": "text", "text": text}});
    if let Some(message) = message {
        value["messageId"] = json!(message);
    }
    value
}

#[test]
fn chunks_of_one_message_share_an_entry() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    apply(
        &mut projector,
        &mut transcript,
        chunk("agent_message_chunk", "a", None),
    );
    apply(
        &mut projector,
        &mut transcript,
        chunk("agent_message_chunk", "b", None),
    );
    apply(
        &mut projector,
        &mut transcript,
        chunk("agent_thought_chunk", "t", None),
    );
    apply(
        &mut projector,
        &mut transcript,
        chunk("agent_message_chunk", "c", None),
    );
    let texts: Vec<_> = transcript
        .entries
        .iter()
        .map(|entry| entry.kind.text().unwrap())
        .collect();
    assert_eq!(texts, ["ab", "t", "c"]);
}

#[test]
fn a_new_message_id_starts_a_new_entry() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    apply(
        &mut projector,
        &mut transcript,
        chunk("user_message_chunk", "one", Some("m1")),
    );
    apply(
        &mut projector,
        &mut transcript,
        chunk("user_message_chunk", "two", Some("m2")),
    );
    assert_eq!(transcript.entries.len(), 2);
}

#[test]
fn closing_ends_the_streaming_entry() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    apply(
        &mut projector,
        &mut transcript,
        chunk("agent_message_chunk", "a", None),
    );
    projector.close();
    apply(
        &mut projector,
        &mut transcript,
        chunk("agent_message_chunk", "b", None),
    );
    assert_eq!(transcript.entries.len(), 2);
}

#[test]
fn tool_updates_merge_into_the_call() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "tool_call",
        "toolCallId": "t1", "title": "Read", "kind": "read", "status": "pending",
        "locations": [{"path": "/a.rs", "line": 3}]}),
    );
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "tool_call_update",
        "toolCallId": "t1", "status": "failed",
        "content": [{"type": "content", "content": {"type": "text", "text": "nope"}}]}),
    );
    let [entry] = transcript.entries.as_slice() else {
        panic!("one entry expected");
    };
    let EntryKind::Tool(tool) = &entry.kind else {
        panic!("tool expected");
    };
    assert_eq!(tool.title, "Read");
    assert_eq!(tool.kind, ToolKind::Read);
    assert_eq!(tool.status, ToolStatus::Failed);
    assert_eq!(tool.locations, ["/a.rs:3"]);
    assert_eq!(
        tool.content,
        [ToolContent::Text {
            text: "nope".to_owned()
        }]
    );
}

#[test]
fn plans_commands_usage_and_titles_are_projected() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "plan", "entries": [
        {"content": "Write tests", "priority": "high", "status": "in_progress"}]}),
    );
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "available_commands_update",
        "availableCommands": [{"name": "review", "description": "Review",
        "input": {"hint": "what to review"}}]}),
    );
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "usage_update",
        "used": 10, "size": 100}),
    );
    // Mode and title updates need a session and modes to apply to.
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "current_mode_update",
        "currentModeId": "plan"}),
    );
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "session_info_update",
        "title": "Fix"}),
    );
    assert_eq!(transcript.plan[0].status, PlanStatus::InProgress);
    assert_eq!(transcript.plan[0].priority, PlanPriority::High);
    assert_eq!(
        transcript.commands[0].hint.as_deref(),
        Some("what to review")
    );
    assert_eq!(
        transcript.usage,
        Some(Usage {
            cost: None,
            size: 100,
            used: 10
        })
    );
    assert!(transcript.modes.is_none() && transcript.session.is_none());

    transcript.session = Some(SessionInfo {
        cwd: None,
        id: "s".to_owned(),
        title: None,
    });
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "session_info_update",
        "title": "Fix"}),
    );
    assert_eq!(transcript.session.unwrap().title.as_deref(), Some("Fix"));
}

#[test]
fn media_are_named_not_embedded() {
    let image: wire::ContentBlock =
        serde_json::from_value(json!({"type": "image", "data": "AAAA", "mimeType": "image/png"}))
            .unwrap();
    assert_eq!(block_text(&image), "[image]");
    let link: wire::ContentBlock = serde_json::from_value(
        json!({"type": "resource_link", "name": "spec", "uri": "file:///spec.md"}),
    )
    .unwrap();
    assert_eq!(block_text(&link), "[spec](file:///spec.md)");
}

#[test]
fn entry_text_is_capped_once_with_a_marker() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    let big = "é".repeat(MAX_ENTRY_TEXT / 3);
    for _ in 0..4 {
        if let Some(change) = projector.text(&transcript, Stream::Agent, None, big.clone()) {
            transcript.apply(&change).unwrap();
        }
    }
    let text = transcript.entries[0].kind.text().unwrap();
    assert!(text.len() <= MAX_ENTRY_TEXT);
    assert!(text.ends_with(TRUNCATED));
    assert_eq!(text.matches(TRUNCATED).count(), 1);
    assert!(
        projector
            .text(&transcript, Stream::Agent, None, "more".to_owned())
            .is_none()
    );
}
