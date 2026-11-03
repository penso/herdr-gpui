//! A sub-agent's call is marked as one, and its own steps are listed in its
//! card rather than in the conversation.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::{Modifiers, TestAppContext};
use herdr_chat_protocol::{Change, Tool, ToolKind, ToolStatus};
use herdr_client::ConnectTarget;

fn tool(id: u64, call: &str, title: &str, kind: ToolKind, parent: Option<&str>) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Tool(Tool {
            call_id: call.into(),
            content: Vec::new(),
            kind,
            locations: Vec::new(),
            parent: parent.map(str::to_owned),
            status: ToolStatus::Completed,
            title: title.into(),
        }),
    }
}

#[gpui::test]
fn a_sub_agents_steps_live_in_its_card(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![
                agent(1, "Delegating."),
                tool(2, "task", "Explore the parser", ToolKind::Subagent, None),
                tool(
                    3,
                    "read",
                    "Read src/parser.rs",
                    ToolKind::Read,
                    Some("task"),
                ),
                tool(
                    4,
                    "grep",
                    "Search for parse",
                    ToolKind::Search,
                    Some("task"),
                ),
            ],
        ),
        cx,
    );
    assert!(cx.debug_bounds("chat-tool-2").is_some());
    assert!(cx.debug_bounds("chat-tool-3").is_none(), "not in the flow");
    assert!(cx.debug_bounds("chat-steps-2").is_none(), "folded");
    let header = cx.debug_bounds("chat-tool-2").unwrap();
    cx.simulate_click(header.center(), Modifiers::default());
    poll(&view, &program, cx);
    let steps = cx.debug_bounds("chat-steps-2").unwrap();
    assert!(steps.top() >= header.bottom());
    assert!(cx.debug_bounds("chat-step-3").is_some());
    assert!(cx.debug_bounds("chat-step-4").is_some());

    // A new step shows in the open card as it comes.
    send(
        &mut stream,
        &ServerMessage::Change {
            change: Change::Push {
                entry: tool(
                    5,
                    "edit",
                    "Edit src/parser.rs",
                    ToolKind::Edit,
                    Some("task"),
                ),
            },
            cursor: cursor(1, 1),
        },
    );
    wait_until(|| {
        poll(&view, &program, cx);
        cx.debug_bounds("chat-step-5").is_some()
    });
    assert!(cx.debug_bounds("chat-tool-5").is_none());
}
