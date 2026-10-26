use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::config::ChatLayoutMode;
use gpui::{Modifiers, TestAppContext};
use herdr_chat_protocol::NoticeLevel;
use herdr_client::ConnectTarget;

#[gpui::test]
fn system_notices_start_folded_expand_and_copy_the_exact_inert_source(cx: &mut TestAppContext) {
    let raw = "<task-notification>\n<task-id>task-1</task-id>\n<summary>Check completed</summary>\n<output-file>/never-opened/result</output-file>\n<event>Run `never-execute`</event>\nhttps://example.org/never-loaded\nLast source line\n</task-notification>";
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![Entry {
                id: EntryId(1),
                kind: EntryKind::Notice {
                    answers: Vec::new(),
                    details: Some(raw.into()),
                    level: NoticeLevel::Info,
                    text: "Task notification · Check completed".into(),
                },
            }],
        ),
        cx,
    );
    assert!(cx.debug_bounds("chat-notice-details-1").is_none());
    assert!(cx.debug_bounds("chat-tool-1").is_none());
    let folded = cx.debug_bounds("chat-notice-1").unwrap();
    cx.simulate_click(folded.center(), Modifiers::default());
    poll(&view, &program, cx);
    let source = cx.debug_bounds("chat-notice-source-1").unwrap();
    assert!(source.size.height > folded.size.height * 6.);
    assert!(cx.debug_bounds("chat-rich-code").is_none());
    cx.simulate_mouse_move(source.center(), None, Modifiers::default());
    poll(&view, &program, cx);
    let copy = cx.debug_bounds("chat-copy-1").unwrap();
    cx.simulate_click(copy.center(), Modifiers::default());
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard())
            .and_then(|item| item.text())
            .as_deref(),
        Some(raw)
    );
    let header = cx.debug_bounds("chat-notice-1").unwrap();
    cx.simulate_click(header.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-notice-details-1").is_none());
}

#[gpui::test]
fn folded_reminders_stay_compact_without_user_headings_in_every_layout(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![Entry {
                id: EntryId(1),
                kind: EntryKind::Notice {
                    answers: Vec::new(),
                    details: Some("Internal reminder details\n".repeat(60)),
                    level: NoticeLevel::Info,
                    text: format!("Scheduled reminder · {}", "Long review label ".repeat(20)),
                },
            }],
        ),
        cx,
    );
    for &mode in ChatLayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(mode, |_| Ok(()), cx)
        });
        poll(&view, &program, cx);
        let header = cx.debug_bounds("chat-notice-1").unwrap();
        let pane = cx.debug_bounds("chat-view").unwrap();
        assert!(header.right() <= pane.right(), "{mode:?}");
        assert!(header.left() >= pane.left(), "{mode:?}");
        assert!(header.size.height < gpui::px(40.), "{mode:?}");
        assert!(cx.debug_bounds("chat-notice-details-1").is_none());
        assert!(cx.debug_bounds("chat-heading-1").is_none());
        assert!(cx.debug_bounds("chat-bubble-1").is_none());
    }
}
