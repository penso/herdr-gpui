//! An opened tool card draws source and diffs with their own copy actions,
//! and folds from anywhere on its header.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::{Modifiers, TestAppContext};
use sheprd_chat_protocol::{Tool, ToolContent, ToolKind, ToolStatus};
use sheprd_client::ConnectTarget;

fn tool(content: Vec<ToolContent>, kind: ToolKind) -> Entry {
    Entry {
        id: EntryId(1),
        kind: EntryKind::Tool(Tool {
            call_id: "call".into(),
            content,
            kind,
            locations: Vec::new(),
            parent: None,
            started_ms: None,
            status: ToolStatus::Completed,
            title: "Short".into(),
        }),
    }
}

#[gpui::test]
fn an_opened_edit_draws_a_diff_with_copy_diff_and_copy_original(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let entry = tool(
        vec![ToolContent::Diff {
            new: "new\n".into(),
            old: Some("old\n".into()),
            path: "src/a.rs".into(),
        }],
        ToolKind::Edit,
    );
    let _stream = attach_with(&view, &program, snapshot(cursor(1, 0), vec![entry]), cx);
    assert!(cx.debug_bounds("chat-copy-diff").is_none());
    let header = cx.debug_bounds("chat-tool-1").unwrap();
    cx.simulate_click(header.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-copy-diff").is_some());
    assert!(cx.debug_bounds("chat-copy-original").is_some());
    assert!(cx.debug_bounds("chat-code-line-0").is_some());
}

#[gpui::test]
fn plain_output_has_one_copy_action(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let entry = tool(
        vec![ToolContent::Text {
            text: "plain output".into(),
        }],
        ToolKind::Execute,
    );
    let _stream = attach_with(&view, &program, snapshot(cursor(1, 0), vec![entry]), cx);
    let header = cx.debug_bounds("chat-tool-1").unwrap();
    cx.simulate_click(header.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-copy-code").is_some());
    assert!(cx.debug_bounds("chat-copy-original").is_none());
}

/// Past a short title the wide source below is the closest text, which must
/// not take the click from the header it landed on.
#[gpui::test]
fn a_click_past_a_short_title_folds_the_opened_card(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let entry = tool(
        vec![ToolContent::Text {
            text: "x".repeat(400),
        }],
        ToolKind::Execute,
    );
    let _stream = attach_with(&view, &program, snapshot(cursor(1, 0), vec![entry]), cx);
    for _ in 0..3 {
        let opened = cx.debug_bounds("chat-tool-details-1").is_some();
        let header = cx.debug_bounds("chat-tool-1").unwrap();
        cx.simulate_click(header.center(), Modifiers::default());
        poll(&view, &program, cx);
        assert_eq!(cx.debug_bounds("chat-tool-details-1").is_some(), !opened);
    }
}
