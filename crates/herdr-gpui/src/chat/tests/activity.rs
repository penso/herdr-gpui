use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::{Modifiers, TestAppContext};
use herdr_chat_protocol::{Tool, ToolContent, ToolKind, ToolStatus};
use herdr_client::ConnectTarget;

#[gpui::test]
fn tools_hide_output_until_expanded_and_collapse_it_again(cx: &mut TestAppContext) {
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
                    content: vec![ToolContent::Text {
                        text: "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight".into(),
                    }],
                    kind: ToolKind::Execute,
                    locations: Vec::new(),
                    parent: None,
                    status: ToolStatus::Completed,
                    title: "Read complete output".into(),
                }),
            }],
        ),
        cx,
    );
    assert!(cx.debug_bounds("chat-tool-details-1").is_none());
    assert!(cx.debug_bounds("chat-tool-content-1-0").is_none());
    assert!(cx.debug_bounds("chat-code-line-0").is_none());
    let header = cx.debug_bounds("chat-tool-1").unwrap();
    cx.simulate_click(header.center(), Modifiers::default());
    poll(&view, &program, cx);
    let content = cx.debug_bounds("chat-tool-content-1-0").unwrap();
    assert!(
        content.size.height > header.size.height * 6.,
        "output beyond the old six-line preview is drawn"
    );
    assert!(cx.debug_bounds("chat-code-line-7").is_some());
    // The expanded code panel is taller than the fixture's list viewport.
    // Tail following can move its disclosure above the visible region; a
    // debug bound still exists for that overdrawn row. Scroll it into view
    // before clicking, as a user must (also covered by the embed tests).
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
    assert!(cx.debug_bounds("chat-tool-details-1").is_none());
    assert!(cx.debug_bounds("chat-tool-content-1-0").is_none());
    assert!(cx.debug_bounds("chat-code-line-0").is_none());
}

#[gpui::test]
fn thoughts_start_folded_and_expand_without_losing_their_text(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![Entry {
                id: EntryId(1),
                kind: EntryKind::Thought {
                    text: "First line\nSecond line\nThird line\nLast line".into(),
                },
            }],
        ),
        cx,
    );
    let folded = cx.debug_bounds("chat-thought-1").unwrap();
    cx.simulate_click(folded.center(), Modifiers::default());
    poll(&view, &program, cx);
    let expanded = cx.debug_bounds("chat-thought-1").unwrap();
    assert!(expanded.size.height > folded.size.height * 2.);
    cx.simulate_click(expanded.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert_eq!(
        cx.debug_bounds("chat-thought-1").unwrap().size.height,
        folded.size.height
    );
}

mod diff_previews;
mod disclosure;
mod tool_previews;
