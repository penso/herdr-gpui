use super::*;
use herdr_chat_protocol::{Tool, ToolKind, ToolStatus};

fn call(id: u64, call: &str, title: &str, parent: Option<&str>) -> Entry {
    entry(
        id,
        EntryKind::Tool(Tool {
            call_id: call.into(),
            content: Vec::new(),
            kind: ToolKind::Subagent,
            locations: Vec::new(),
            parent: parent.map(str::to_owned),
            status: ToolStatus::InProgress,
            title: title.into(),
        }),
    )
}

#[test]
fn a_sub_agents_own_steps_stay_inside_its_call() {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![
        entry(
            1,
            EntryKind::User {
                text: "delegate".into(),
                files: 0,
            },
        ),
        call(2, "task", "Explore the parser", None),
        call(3, "inner", "Read src/secret_step.rs", Some("task")),
    ];
    let view = View {
        transcript,
        connected: true,
    };
    let html = chat(&workspace(vec![]), &row("p1"), &view, 0).into_string();
    assert!(html.contains("Explore the parser"));
    assert!(!html.contains("secret_step"));
}
