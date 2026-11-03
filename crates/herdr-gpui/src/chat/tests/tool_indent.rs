//! A tool call nests in the bubbles layout: the header sits past the
//! prose edge, and the call's body (its preview lines, and its opened
//! details) sits past the header. Other layouts keep both flush.

use super::{
    overlay::{attach_with, open, poll},
    timeline::conversation,
    *,
};
use crate::config::ChatLayoutMode;
use gpui::{TestAppContext, px};
use herdr_client::ConnectTarget;

#[gpui::test]
fn bubbles_nest_a_tools_body_under_its_header(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, conversation(false), cx);
    let switch = |mode, cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| {
            view.set_chat_layout_with(mode, |_| Ok(()), cx)
        })
    };
    switch(ChatLayoutMode::Bubbles, cx);
    poll(&view, &program, cx);
    let tool = cx.debug_bounds("chat-tool-3").unwrap();
    let diff = cx.debug_bounds("chat-tool-diff-3-0").unwrap();
    assert_eq!(
        diff.left(),
        tool.left() + px(28.),
        "the body sits past the header"
    );

    switch(ChatLayoutMode::Timeline, cx);
    poll(&view, &program, cx);
    let tool = cx.debug_bounds("chat-tool-3").unwrap();
    let diff = cx.debug_bounds("chat-tool-diff-3-0").unwrap();
    assert_eq!(diff.left(), tool.left(), "the timeline keeps both flush");
}
