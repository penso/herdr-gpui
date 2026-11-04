#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    control::protocol::{Caller, chat::Reply},
    sidebar::layout_tests::fixture_window,
};

fn call(op: ChatOp) -> Call<ChatOp> {
    Call {
        caller: Caller::default(),
        op,
    }
}

fn code(response: &Response) -> Option<ErrorCode> {
    match response {
        Response::Error { code, .. } => Some(*code),
        _ => None,
    }
}

#[gpui::test]
fn chat_requests_need_a_window(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        assert_eq!(
            code(&serve(call(ChatOp::Status), cx)),
            Some(ErrorCode::NoWindow)
        );
    });
}

#[gpui::test]
fn a_window_without_chats_lists_none_and_shows_none(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(fixture_window);
    cx.cx.update(|cx| {
        assert_eq!(
            serve(call(ChatOp::Status), cx),
            Response::Chat {
                reply: Reply::Chats { chats: Vec::new() }
            }
        );
        let show = serve(
            call(ChatOp::View {
                switch: crate::control::protocol::app::Switch::On,
                pane: Some("w1:p1".into()),
            }),
            cx,
        );
        assert_eq!(code(&show), Some(ErrorCode::NotFound));
    });
}

#[gpui::test]
fn a_new_chat_names_a_known_agent_and_needs_herdr_acp(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.cx.update(|cx| {
        let unknown = serve(
            call(ChatOp::New {
                agent: "cursor".into(),
                pane: None,
            }),
            cx,
        );
        assert_eq!(code(&unknown), Some(ErrorCode::InvalidRequest));
    });
    view.update(cx, |view, _| {
        view.chats.program = crate::chat::Program::Missing;
    });
    cx.cx.update(|cx| {
        let missing = serve(
            call(ChatOp::New {
                agent: "claude".into(),
                pane: None,
            }),
            cx,
        );
        assert_eq!(code(&missing), Some(ErrorCode::Unsupported));
    });
}
