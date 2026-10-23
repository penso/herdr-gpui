#![allow(clippy::unwrap_used)]
use super::*;

mod chat_lines;

#[test]
fn output_is_whole_and_only_a_terminal_loses_controls() {
    let patch = format!("{}\r\n\u{1b}[31mred\u{1b}[0m\n", "+".repeat(2 << 20));
    let piped = for_terminal(&patch, false);
    assert_eq!(piped, patch, "a pipe gets the output unchanged and whole");
    let shown = for_terminal(&patch, true);
    assert_eq!(shown.chars().filter(|c| *c == '+').count(), 2 << 20);
    assert!(!shown.contains('\u{1b}') && !shown.contains('\r'));
    assert!(shown.ends_with("[31mred[0m\n"));
}

#[test]
fn a_plain_launch_asks_for_the_frontmost_window_as_app_focus_does() {
    let Request::App(Call { caller, op }) = focus_request() else {
        panic!("not an app request");
    };
    assert_eq!(op, crate::control::protocol::app::AppOp::Focus);
    // No pane, workspace, or daemon of a terminal the launch came from: the
    // shell starts it, so the frontmost window comes forward.
    assert_eq!(caller, Caller::default());
}

#[test]
fn every_error_code_has_one_exit_status() {
    let rejected = |code| {
        crate::Error::from(crate::control::Error::Rejected {
            code,
            message: String::new(),
        })
    };
    for (code, expected) in [
        (ErrorCode::InvalidUrl, exit::USAGE),
        (ErrorCode::NotFound, exit::NOT_FOUND),
        (ErrorCode::WorkspaceNotFound, exit::NOT_FOUND),
        (ErrorCode::NoWindow, exit::NOT_FOUND),
        (ErrorCode::Busy, exit::BUSY),
        (ErrorCode::WindowBusy, exit::BUSY),
        (ErrorCode::TabLimit, exit::BUSY),
        (ErrorCode::Timeout, exit::TIMEOUT),
        (ErrorCode::Unsupported, exit::UNSUPPORTED),
        (ErrorCode::InvalidRequest, exit::REFUSED),
        (ErrorCode::Failed, exit::REFUSED),
    ] {
        assert_eq!(status(&rejected(code)), expected, "{code:?}");
    }
}

#[test]
fn every_printed_event_starts_with_the_cursor_to_resume_after_it() {
    let event = crate::control::protocol::events::Event {
        seq: 3,
        at: 0,
        kind: crate::control::EventKind::TabOpened,
        subject: "12".into(),
        detail: Some("w_1".into()),
    };
    assert_eq!(events::line(&event, Some(77)), "77:3 tab_opened 12 w_1");
    assert_eq!(events::line(&event, None), "3 tab_opened 12 w_1");
}

#[test]
fn an_empty_patch_is_empty_in_a_pipe_and_said_on_a_terminal() {
    let mut output = Output::bytes(Vec::new(), &serde_json::json!({})).unwrap();
    assert!(output.text.is_empty());
    output.text = "No changes".into();
    assert_eq!(output.bytes.as_deref(), Some(&[][..]));
    assert_eq!(for_terminal(&output.text, true), "No changes");
}
