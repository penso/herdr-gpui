//! The iA layout: one column, prompts hung from a gutter mark, and the
//! agent's prose aligned with them.

use super::{
    overlay::{attach_with, open, poll},
    timeline::conversation,
    *,
};
use crate::config::ChatLayoutMode;
use gpui::{TestAppContext, px};
use herdr_client::ConnectTarget;

#[gpui::test]
fn prompts_steps_and_prose_share_one_column(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, conversation(false), cx);
    view.update(cx, |view, cx| {
        view.set_chat_layout_with(ChatLayoutMode::IA, |_| Ok(()), cx)
    });
    poll(&view, &program, cx);
    let prompt = cx.debug_bounds("chat-ia-1").unwrap();
    let prose = cx.debug_bounds("chat-ia-2").unwrap();
    let step = cx.debug_bounds("chat-ia-3").unwrap();
    assert_eq!(prompt.left(), prose.left());
    assert_eq!(step.left(), prose.left());
    assert!(prompt.bottom() <= prose.top());
    // The prompt's mark hangs in the gutter, left of the text.
    let mark = cx.debug_bounds("chat-ia-mark-1").unwrap();
    let gutter = view.read_with(cx, |view, _| view.chat_text_size()) * 1.6;
    // The row's padding, then the gutter, then the text.
    let text = prompt.left() + px(16. + gutter);
    assert!(
        mark.left() >= prompt.left() && mark.right() <= text,
        "{mark:?} in {prompt:?}"
    );
    assert!(
        cx.debug_bounds("chat-ia-mark-2").is_none(),
        "only prompts take a mark"
    );
}
