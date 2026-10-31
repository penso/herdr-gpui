//! A folded tool call shows a concise change summary. Its actual output is
//! drawn and selectable only after opening the disclosure.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::chat::selection::{Cell, Region};
use gpui::{Modifiers, TestAppContext};
use herdr_chat_protocol::{Tool, ToolContent, ToolKind, ToolStatus};
use herdr_client::ConnectTarget;

#[gpui::test]
fn folded_tools_keep_diff_counts_and_disclose_output_on_click(cx: &mut TestAppContext) {
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
                            text: "compiled\nline two\nline three\nline four\nline five\nline six"
                                .into(),
                        },
                    ],
                    kind: ToolKind::Edit,
                    locations: vec!["/work/src/lib.rs".into()],
                    parent: None,
                    status: ToolStatus::Completed,
                    title: "Edit".into(),
                }),
            }],
        ),
        cx,
    );
    assert!(cx.debug_bounds("chat-tool-details-1").is_none());
    assert!(cx.debug_bounds("chat-tool-diff-1-0").is_some());
    assert!(cx.debug_bounds("chat-tool-content-1-0").is_none());
    assert!(cx.debug_bounds("chat-tool-content-1-1").is_none());
    view.read_with(cx, |view, _| {
        let text = view.chats.pane("w1:p1").unwrap().prepared.text();
        let region = Region::ToolPreview(1);
        assert!(text.key(EntryId(1), region, Cell::row(0)).is_none());
    });
    // Make the target visibly clickable even if this fixture changes size.
    view.update(cx, |view, _| {
        let pane = view.chats.pane("w1:p1").unwrap();
        pane.list.scroll_to(gpui::ListOffset {
            item_ix: 0,
            offset_in_item: gpui::px(0.),
        });
        pane.glide.user_scrolled();
    });
    poll(&view, &program, cx);
    let header = cx.debug_bounds("chat-tool-1").unwrap();
    let viewport = view.read_with(cx, |view, _| {
        view.chats.pane("w1:p1").unwrap().list.viewport_bounds()
    });
    assert!(header.top() >= viewport.top() && header.bottom() <= viewport.bottom());
    cx.simulate_click(header.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-tool-details-1").is_some());
    assert!(cx.debug_bounds("chat-tool-diff-1-0").is_some());
    // Expansion pushes the second panel below the viewport. Reveal it before
    // checking its visible copy text; outer height also includes wrapping
    // labels and a truncation hint, so it need not increase on expansion.
    view.update(cx, |view, _| {
        let pane = view.chats.pane("w1:p1").unwrap();
        pane.list.scroll_to(gpui::ListOffset {
            item_ix: pane.list.item_count(),
            offset_in_item: gpui::px(0.),
        });
        pane.glide.user_scrolled();
    });
    poll(&view, &program, cx);
    let expanded_output = cx.debug_bounds("chat-tool-content-1-1").unwrap();
    assert!(expanded_output.size.height > gpui::px(0.));
    view.read_with(cx, |view, _| {
        let text = view.chats.pane("w1:p1").unwrap().prepared.text();
        let region = Region::ToolPreview(1);
        let last = text.key(EntryId(1), region, Cell::row(5)).unwrap();
        assert_eq!(text.segment(last).unwrap().text(), "line six");
    });
}
