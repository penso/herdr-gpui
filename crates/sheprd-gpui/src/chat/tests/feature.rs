//! ACP chats ship on, and a person can turn them off: off, nothing attaches,
//! nothing is offered or looked for, and a start is refused.

use super::{
    overlay::{open, poll},
    *,
};
use crate::{chat::start, sidebar::layout_tests::full_draw};
use gpui::TestAppContext;
use sheprd_chat_protocol::Agent;
use sheprd_client::ConnectTarget;

#[gpui::test]
fn off_no_chat_attaches_and_none_starts(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    view.update(cx, |view, _| {
        view.config.features.acp_chats = false;
        view.chats.program = start::Program::Found("/opt/herdr-acp".into());
    });
    let mut dialled = 0;
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.poll_chats_with(
                &mut |_, _, _| {
                    dialled += 1;
                    Err(Error::Closed)
                },
                window,
                cx,
            );
        });
        full_draw(window, cx).clear(cx);
    });
    assert_eq!(dialled, 0);
    assert!(cx.debug_bounds("chat-view").is_none());
    view.update(cx, |view, _| {
        assert!(!view.can_start_chat());
        assert!(view.chat_agents().is_empty());
        assert!(matches!(
            view.start_chat(Agent::Opencode, None),
            Err(start::Error::Disabled)
        ));
    });

    // Turned on, the same pane attaches.
    let program = Program::new();
    view.update(cx, |view, _| view.config.features.acp_chats = true);
    poll(&view, &program, cx);
    let (_stream, _) = program.accept();
}
