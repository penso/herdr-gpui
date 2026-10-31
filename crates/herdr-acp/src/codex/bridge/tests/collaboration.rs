use super::*;

#[test]
fn both_collaboration_call_spellings_describe_the_action_with_separate_prompt_details() {
    let mut bridge = opened();
    for kind in ["collabToolCall", "collabAgentToolCall"] {
        for (tool, label) in [
            ("spawnAgent", "Review the parser"),
            ("sendInput", "Message a sub-agent"),
            ("resumeAgent", "Resume a sub-agent"),
            ("wait", "Wait for sub-agents"),
            ("closeAgent", "Close a sub-agent"),
            ("sendMessage", "Message a sub-agent"),
            ("followupTask", "Message a sub-agent"),
            ("interruptAgent", "Interrupt a sub-agent"),
            ("listAgents", "List sub-agents"),
        ] {
            let outs = notify(
                &mut bridge,
                "item/started",
                json!({"threadId": THREAD, "item": {
                    "type": kind, "id": "collab", "tool": tool,
                    "status": "inProgress", "prompt": "Review the parser\nKeep the output concise.",
                }}),
            );
            let call = update(&outs[0]);
            assert_eq!(call["title"], label);
            assert_eq!(call["kind"], "think");
            assert_eq!(call["status"], "in_progress");
            assert_eq!(
                call["content"][0]["content"]["text"],
                "Review the parser\nKeep the output concise."
            );
        }
    }
    let old_activity = notify(
        &mut bridge,
        "item/started",
        json!({"threadId": THREAD, "item": {
            "type": "subAgentActivity", "id": "child", "prompt": "Reviewing tests",
        }}),
    );
    assert!(
        old_activity.is_empty(),
        "activity reports are not duplicate calls"
    );
}

#[test]
fn collaboration_completion_describes_the_wrapper_without_inventing_child_completion() {
    let mut bridge = opened();
    for (status, expected) in [
        ("inProgress", "in_progress"),
        ("completed", "completed"),
        ("failed", "failed"),
        ("interrupted", "failed"),
    ] {
        let outs = notify(
            &mut bridge,
            "item/completed",
            json!({"threadId": THREAD, "item": {
                "type": "collabToolCall", "id": "collab", "tool": "spawnAgent",
                "status": status, "newThreadId": "child", "agentStatus": "running",
            }}),
        );
        assert_eq!(outs.len(), 1);
        let call = update(&outs[0]);
        assert_eq!(call["toolCallId"], "collab");
        assert_eq!(call["title"], "Start a sub-agent");
        assert_eq!(call["status"], expected);
        assert_eq!(call["content"], json!([]));
    }
}

#[test]
fn collaboration_prompt_details_and_title_are_bounded_independently() {
    let mut bridge = opened();
    let outs = notify(
        &mut bridge,
        "item/started",
        json!({"threadId": THREAD, "item": {
            "type": "collabAgentToolCall", "id": "collab", "tool": "spawnAgent",
            "prompt": "🙂".repeat(crate::project::MAX_ENTRY_TEXT),
        }}),
    );
    let call = update(&outs[0]);
    let title = call["title"].as_str().unwrap();
    assert!(title.len() <= 256);
    assert!(title.ends_with('…'));
    let text = call["content"][0]["content"]["text"].as_str().unwrap();
    assert!(text.len() <= crate::project::MAX_ENTRY_TEXT);
    assert!(text.ends_with("… (truncated)"));
}
