//! The OpenCode layout: prompts and open calls as blocks on one text edge,
//! finished calls on one line, and what needs attention left in view.

use super::{
    overlay::{attach_with, open, poll},
    timeline::conversation,
    *,
};
use crate::config::ChatLayoutMode;
use gpui::{TestAppContext, VisualTestContext};
use sheprd_client::ConnectTarget;

fn opencode(
    view: &gpui::Entity<crate::SheprdWindow>,
    program: &Program,
    cx: &mut VisualTestContext,
) {
    view.update(cx, |view, cx| {
        view.set_chat_layout_with(ChatLayoutMode::OpenCode, |_| Ok(()), cx)
    });
    poll(view, program, cx);
}

#[gpui::test]
fn a_finished_edit_folds_to_one_line_with_its_totals(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, conversation(false), cx);
    opencode(&view, &program, cx);

    assert!(cx.debug_bounds("chat-tool-totals-3").is_some());
    assert!(
        cx.debug_bounds("chat-tool-diff-3-0").is_none(),
        "the file's own row waits until the call is opened"
    );
}

#[gpui::test]
fn opening_a_finished_call_trades_its_totals_for_its_files(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, conversation(false), cx);
    opencode(&view, &program, cx);
    let folded = cx.debug_bounds("chat-opencode-3").unwrap();

    let pane = view.read_with(cx, |view, _| {
        view.chats.panes().next().map(|p| p.target.pane_id.clone())
    });
    view.update(cx, |view, cx| {
        let chat = pane.as_deref().and_then(|pane| view.chats.pane_mut(pane));
        if let Some(chat) = chat {
            chat.toggle_expanded(EntryId(3));
        }
        cx.notify();
    });
    poll(&view, &program, cx);

    assert!(cx.debug_bounds("chat-tool-totals-3").is_none());
    assert!(cx.debug_bounds("chat-tool-diff-3-0").is_some());
    assert!(cx.debug_bounds("chat-opencode-3").unwrap().size.height > folded.size.height);
}

#[gpui::test]
fn prompts_and_folded_calls_share_a_text_edge(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, conversation(false), cx);
    opencode(&view, &program, cx);

    let prompt = cx.debug_bounds("chat-opencode-1").unwrap();
    let call = cx.debug_bounds("chat-opencode-3").unwrap();
    assert_eq!(prompt.left(), call.left());
    assert!(cx.debug_bounds("chat-opencode-status").is_some());
}

#[gpui::test]
fn other_layouts_keep_a_finished_calls_file_rows(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, conversation(false), cx);
    poll(&view, &program, cx);

    assert!(cx.debug_bounds("chat-tool-diff-3-0").is_some());
    assert!(cx.debug_bounds("chat-tool-totals-3").is_none());
    assert!(cx.debug_bounds("chat-opencode-status").is_none());
}
