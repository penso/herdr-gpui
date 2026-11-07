#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::protocol::app::Switch;

fn call<T>(op: T) -> Call<T> {
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
fn status_answers_without_a_window(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let Response::Status(status) = status(&Caller::default(), cx) else {
            panic!("status answered something else");
        };
        assert_eq!(status.protocol, PROTOCOL_VERSION);
        assert_eq!(status.pid, std::process::id());
        assert!(status.windows.is_empty());
    });
}

#[gpui::test]
fn window_requests_without_a_window_are_refused_by_code(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let fullscreen = window(
            call(WindowOp::Fullscreen {
                switch: Switch::Toggle,
            }),
            cx,
        );
        assert_eq!(code(&fullscreen), Some(ErrorCode::NoWindow));
        let named = window(
            Call {
                caller: Caller {
                    window: Some(42),
                    ..Caller::default()
                },
                op: WindowOp::Focus,
            },
            cx,
        );
        assert_eq!(code(&named), Some(ErrorCode::NotFound));
        assert!(matches!(
            window(call(WindowOp::List), cx),
            Response::App {
                reply: Reply::Windows { windows }
            } if windows.is_empty()
        ));
        // Identifiers are echoed in answers, so a control character is refused.
        let invalid = ui(
            Call {
                caller: Caller {
                    workspace_id: Some("w\u{1b}".into()),
                    ..Caller::default()
                },
                op: UiOp::Sidebar { switch: Switch::On },
            },
            cx,
        );
        assert_eq!(code(&invalid), Some(ErrorCode::InvalidRequest));
    });
}

#[gpui::test]
fn commands_are_run_by_their_keybinding_names_only(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let unknown = command(
            call(CommandOp::Run {
                name: "rm -rf".into(),
            }),
            cx,
        );
        assert_eq!(code(&unknown), Some(ErrorCode::NotFound));
        let known = command(
            call(CommandOp::Run {
                name: "toggle_sidebar".into(),
            }),
            cx,
        );
        assert_eq!(code(&known), Some(ErrorCode::NoWindow));
    });
}

#[gpui::test]
fn a_new_window_joins_the_callers_own_session(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let pane = Caller {
            daemon_socket: Some("/tmp/herdr-foo.sock".into()),
            ..Caller::default()
        };
        // No window knows that daemon: it is attached to by its socket, never
        // swapped for the default session.
        assert_eq!(
            new_window_target(&pane, cx).unwrap(),
            ConnectTarget::Socket("/tmp/herdr-foo.sock".into())
        );
        // Outside Herdr, with no window, the local default.
        assert_eq!(
            new_window_target(&Caller::default(), cx).unwrap(),
            ConnectTarget::Local
        );
        // A named window must exist: never a guess at another session.
        let named = Caller {
            window: Some(42),
            ..pane
        };
        let refused = new_window_target(&named, cx).unwrap_err();
        assert_eq!(code(&refused), Some(ErrorCode::NotFound));
    });
}

mod open;
mod ui_panel;
