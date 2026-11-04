use super::*;
use sheprd_chat_protocol::ToolContent;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

fn kinds(harness: &Harness) -> Vec<&'static str> {
    harness
        .transcript()
        .entries
        .iter()
        .map(|entry| match &entry.kind {
            EntryKind::User { .. } => "user",
            EntryKind::Agent { .. } => "agent",
            EntryKind::Image { .. } => "image",
            EntryKind::Tool(_) => "tool",
            EntryKind::Turn(_) => "turn",
            _ => "other",
        })
        .collect()
}

#[test]
fn a_message_s_linked_pictures_follow_it_once_it_closes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("chart.png"), PNG).unwrap();
    let mut harness = ready();
    harness.session.config.cwd = dir.path().to_owned();
    let id = prompt(&mut harness, 0, "draw it");
    harness.agent_says("Here it is: ![chart](chart.png)");
    harness.agent_says(" and ![gone](missing.png)");
    assert_eq!(kinds(&harness), ["user", "agent"], "still streaming");
    harness.update(json!({"sessionUpdate": "tool_call", "toolCallId": "t",
        "title": "ls", "kind": "execute", "status": "pending"}));
    assert_eq!(kinds(&harness), ["user", "agent", "image", "tool"]);
    harness.respond(id, json!({"stopReason": "end_turn"}));
    assert_eq!(kinds(&harness), ["user", "agent", "image", "tool", "turn"]);
}

#[test]
fn a_finished_tool_call_shows_the_picture_it_wrote() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("plot.png"), PNG).unwrap();
    let mut harness = ready();
    harness.session.config.cwd = dir.path().to_owned();
    let _id = prompt(&mut harness, 0, "plot");
    harness.update(json!({"sessionUpdate": "tool_call", "toolCallId": "w",
        "title": "Write plot.png", "kind": "edit", "status": "in_progress",
        "locations": [{"path": dir.path().join("plot.png")}]}));
    let images = |harness: &Harness| {
        harness
            .transcript()
            .entries
            .iter()
            .filter_map(|entry| match &entry.kind {
                EntryKind::Tool(tool) => Some(
                    tool.content
                        .iter()
                        .filter(|content| matches!(content, ToolContent::Image { .. }))
                        .count(),
                ),
                _ => None,
            })
            .sum::<usize>()
    };
    assert_eq!(images(&harness), 0, "not finished");
    harness.update(
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "w",
        "status": "completed"}),
    );
    assert_eq!(images(&harness), 1);
    harness.update(
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "w",
        "status": "completed"}),
    );
    assert_eq!(images(&harness), 1, "read once");
}
