use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::{TestAppContext, px};
use herdr_client::ConnectTarget;

#[gpui::test]
fn ordinary_and_linked_prose_wrap_inside_the_chat_column(cx: &mut TestAppContext) {
    let words = "A long paragraph must wrap inside its available column without clipping the words on the right. ".repeat(6);
    for text in [
        words.clone(),
        format!("[Read the guide](https://example.org). {words}"),
    ] {
        let (view, _peer, cx) = open(cx, ConnectTarget::Local);
        let program = Program::new();
        let _stream = attach_with(
            &view,
            &program,
            snapshot(cursor(1, 0), vec![agent(1, &text)]),
            cx,
        );
        poll(&view, &program, cx);
        let prose = cx.debug_bounds("chat-prose").unwrap();
        let chat = cx.debug_bounds("chat-view").unwrap();
        assert!(prose.right() <= chat.right(), "prose exceeds the pane");
        assert!(prose.left() >= chat.left(), "prose begins outside the pane");
        assert!(
            prose.size.height >= px(48.),
            "long prose stayed on one clipped line"
        );
    }
}
