//! A folded tool call still shows what it changed and the start of its
//! output; opening it trades the preview for the full text.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::{Modifiers, TestAppContext};
use herdr_chat_protocol::{Tool, ToolContent, ToolKind, ToolStatus};
use herdr_client::ConnectTarget;

#[gpui::test]
fn folded_tools_keep_diff_counts_and_an_output_preview(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![Entry {
                id: EntryId(1),
                kind: EntryKind::Tool(Tool {
                    call_id: "call".into(),
                    content: vec![
                        ToolContent::Diff {
                            path: "/work/src/lib.rs".into(),
                            old: Some("a\nb\n".into()),
                            new: "a\nc\nd\n".into(),
                        },
                        ToolContent::Text {
                            text: "compiled".into(),
                        },
                    ],
                    kind: ToolKind::Edit,
                    locations: vec!["/work/src/lib.rs".into()],
                    status: ToolStatus::Completed,
                    title: "Edit".into(),
                }),
            }],
        ),
        cx,
    );
    assert!(cx.debug_bounds("chat-tool-details-1").is_none());
    assert!(cx.debug_bounds("chat-tool-diff-1-0").is_some());
    assert!(cx.debug_bounds("chat-tool-preview-1").is_some());
    let header = cx.debug_bounds("chat-tool-1").unwrap();
    cx.simulate_click(header.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-tool-details-1").is_some());
    assert!(cx.debug_bounds("chat-tool-diff-1-0").is_some());
    assert!(cx.debug_bounds("chat-tool-preview-1").is_none());
}
