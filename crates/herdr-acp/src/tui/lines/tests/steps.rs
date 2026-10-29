//! A sub-agent's own steps are drawn by Herdr UI under its call; the
//! terminal shows the call alone.

use super::*;

fn call(title: &str, parent: Option<&str>) -> Entry {
    Entry {
        id: EntryId(1),
        kind: EntryKind::Tool(Tool {
            call_id: "c".to_owned(),
            content: Vec::new(),
            kind: ToolKind::Read,
            locations: Vec::new(),
            parent: parent.map(str::to_owned),
            status: ToolStatus::Completed,
            title: title.to_owned(),
        }),
    }
}

#[test]
fn a_sub_agents_step_draws_no_line() {
    assert!(entry(&call("Read a.rs", Some("task")), 40).is_empty());
    assert!(!entry(&call("Read a.rs", None), 40).is_empty());
}
