//! Tools that drive sessions, agents, and plans read as what they ask for.

use super::*;

fn check(name: &str, input: Value, kind: ToolKind, title: &str) {
    let described = describe(name, &input);
    assert_eq!(described.kind, kind, "{name}");
    assert_eq!(described.title, title, "{name}");
    assert!(described.locations.is_empty(), "{name}");
}

#[test]
fn command_sessions_and_scripts_name_the_session_without_reading_the_source() {
    let cases = [
        ("exec", json!({"code": "run('rm -rf /')"}), "Tool script"),
        (
            "functions.write_stdin",
            json!({"session_id": 7, "chars": "y\n"}),
            "Send input to command session 7",
        ),
        (
            "write_stdin",
            json!({"session_id": "7", "chars": ""}),
            "Poll command session 7",
        ),
        ("wait", json!({"cell_id": 3}), "Wait for script cell 3"),
        (
            "wait",
            json!({"cell_id": "c3", "terminate": true}),
            "Stop script cell c3",
        ),
    ];
    for (name, input, title) in cases {
        check(name, input, ToolKind::Execute, title);
    }
}

#[test]
fn an_unusable_id_keeps_the_tools_name() {
    for id in [
        json!(""),
        json!("a\u{1b}b"),
        json!("a".repeat(MAX_HANDLE + 1)),
    ] {
        let input = json!({"session_id": id});
        check("write_stdin", input, ToolKind::Execute, "write_stdin");
    }
    check("wait", json!({}), ToolKind::Execute, "wait");
}

#[test]
fn agent_and_plan_tools_name_their_target_when_logged() {
    let cases = [
        (
            "collaboration.send_message",
            json!({"target": "reviewer"}),
            "Message agent: reviewer",
        ),
        ("send_message", json!({}), "Message agent"),
        (
            "followup_task",
            json!({"target": "reviewer"}),
            "Continue agent: reviewer",
        ),
        (
            "interrupt_agent",
            json!({"target": "reviewer"}),
            "Interrupt agent: reviewer",
        ),
        ("wait_agent", json!({}), "Wait for an agent update"),
        ("list_agents", json!({}), "List agents"),
        ("TaskOutput", json!({}), "Read task output"),
        ("TaskStop", json!({}), "Stop task"),
        ("TodoWrite", json!({}), "Update plan"),
        ("update_plan", json!({}), "Update plan"),
        ("create_goal", json!({}), "Set goal"),
        ("update_goal", json!({}), "Update goal"),
        ("get_goal", json!({}), "Read goal"),
    ];
    for (name, input, title) in cases {
        check(name, input, ToolKind::Think, title);
    }
}
