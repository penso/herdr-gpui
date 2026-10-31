use super::*;
use crate::chat::selection::{Caret, Cell, Region};
use core::prelude::v1::test;

#[gpui::test]
fn arrow_and_selectable_header_toggle_short_tool_details_once(cx: &mut TestAppContext) {
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
                    parent: None,
                    call_id: "synthetic-short-script".into(),
                    content: vec![
                        ToolContent::Text {
                            text: "const result = await tools.inspect();\ntext(result);".into(),
                        },
                        ToolContent::Text {
                            text: "Synthetic result: two rows inspected.".into(),
                        },
                    ],
                    kind: ToolKind::Execute,
                    locations: Vec::new(),
                    status: ToolStatus::Completed,
                    title: "Inspect synthetic script".into(),
                }),
            }],
        ),
        cx,
    );
    timeline::fit_conversation(&view, &program, cx);
    let label = view.read_with(cx, |view, _| {
        view.chats
            .pane("w1:p1")
            .unwrap()
            .prepared
            .text()
            .key(EntryId(1), Region::Label, Cell::default())
            .unwrap()
    });
    let from = selection::position(
        &view,
        Caret {
            key: label,
            offset: 0,
        },
        cx,
    );
    let to = selection::position(
        &view,
        Caret {
            key: label,
            offset: 7,
        },
        cx,
    );
    selection::drag(cx, from, to);
    poll(&view, &program, cx);
    cx.simulate_keystrokes(selection::copy_key());
    assert_eq!(selection::clipboard(cx).as_deref(), Some("Inspect"));
    assert!(cx.debug_bounds("chat-tool-details-1").is_none());
    assert!(cx.debug_bounds("chat-tool-content-1-0").is_none());
    assert!(cx.debug_bounds("chat-tool-content-1-1").is_none());

    let toggle = cx.debug_bounds("chat-tool-toggle-1").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-tool-details-1").is_some());
    assert!(cx.debug_bounds("chat-tool-content-1-0").is_some());
    assert!(cx.debug_bounds("chat-tool-content-1-1").is_some());
    view.read_with(cx, |view, _| {
        assert!(
            view.chats
                .pane("w1:p1")
                .unwrap()
                .expanded
                .contains(&EntryId(1))
        );
    });

    // Use the shaped selectable label, rather than a blank part of the header.
    // A plain click must toggle once; the preceding drag must only select text.
    let label = view.read_with(cx, |view, _| {
        view.chats
            .pane("w1:p1")
            .unwrap()
            .prepared
            .text()
            .key(EntryId(1), Region::Label, Cell::default())
            .unwrap()
    });
    let label = selection::position(
        &view,
        Caret {
            key: label,
            offset: 3,
        },
        cx,
    );
    cx.simulate_click(label, Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-tool-details-1").is_none());
    assert!(cx.debug_bounds("chat-tool-content-1-0").is_none());
    assert!(cx.debug_bounds("chat-tool-content-1-1").is_none());
    view.read_with(cx, |view, _| {
        assert!(
            !view
                .chats
                .pane("w1:p1")
                .unwrap()
                .expanded
                .contains(&EntryId(1))
        );
    });

    let toggle = cx.debug_bounds("chat-tool-toggle-1").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-tool-content-1-0").is_some());
    assert!(cx.debug_bounds("chat-tool-content-1-1").is_some());
}
