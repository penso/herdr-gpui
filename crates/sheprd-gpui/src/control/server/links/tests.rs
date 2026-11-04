#![allow(clippy::unwrap_used)]
use super::*;
use crate::{control::protocol::Caller, sidebar::layout_tests::fixture_window};
use gpui::TestAppContext;

fn link(url: &str) -> Call<LinkOp> {
    Call {
        caller: Caller::default(),
        op: LinkOp { url: url.into() },
    }
}

fn code(response: &Response) -> Option<ErrorCode> {
    match response {
        Response::Error { code, .. } => Some(*code),
        _ => None,
    }
}

#[gpui::test]
fn links_are_parsed_again_and_only_show_things(cx: &mut TestAppContext) {
    cx.add_window(fixture_window);
    cx.update(|cx| {
        // Any local process can send one, so the app checks it too.
        for url in [
            "herdr-gpui://run?command=calc",
            "herdr-gpui://open?path=relative",
            "https://example.com",
        ] {
            assert_eq!(
                code(&serve(link(url), cx)),
                Some(ErrorCode::InvalidRequest),
                "{url}"
            );
        }
        assert_eq!(
            code(&serve(link("herdr-gpui://device?id=ssh:nowhere"), cx)),
            Some(ErrorCode::NotFound)
        );
        assert!(matches!(
            serve(link("herdr-gpui://device?id=local"), cx),
            Response::Done { .. }
        ));
        // Posted by no window of this app.
        assert_eq!(
            code(&serve(
                link("herdr-gpui://notification?tag=herdr:local:boot:w1:p1"),
                cx
            )),
            Some(ErrorCode::NotFound)
        );
    });
}

#[gpui::test]
fn opening_a_checkout_needs_this_computers_herdr(cx: &mut TestAppContext) {
    let handle = cx.add_window(fixture_window);
    let path = if cfg!(windows) {
        r"C:\checkout"
    } else {
        "/checkout"
    };
    handle
        .update(cx, |view, _, _| {
            view.live.status = crate::state::ConnectionStatus::Disconnected;
        })
        .unwrap();
    cx.update(|cx| {
        assert_eq!(
            code(&serve(link(&Link::Open { path: path.into() }.to_url()), cx)),
            Some(ErrorCode::Busy)
        );
    });
}

#[gpui::test]
fn a_checkout_link_uses_a_window_connected_to_this_computer(cx: &mut TestAppContext) {
    use crate::state::ConnectionStatus;
    let status = |cx: &mut TestAppContext,
                  handle: gpui::WindowHandle<crate::SheprdWindow>,
                  status: ConnectionStatus| {
        handle
            .update(cx, |view, _, _| view.live.status = status)
            .unwrap();
    };
    let detached = cx.add_window(fixture_window);
    status(cx, detached, ConnectionStatus::Disconnected);
    let connected = cx.add_window(fixture_window);
    status(cx, connected, ConnectionStatus::Connected);
    let path = if cfg!(windows) {
        r"C:\checkout"
    } else {
        "/checkout"
    };
    let url = Link::Open { path: path.into() }.to_url();
    // The detached window cannot open it, but the other one can.
    let response = cx.update(|cx| serve(link(&url), cx));
    assert!(matches!(response, Response::Done { .. }), "{response:?}");
    // `--window` names exactly that window, even a detached one.
    let mut call = link(&url);
    call.caller.window = Some(detached.window_id().as_u64());
    cx.update(|cx| assert_eq!(code(&serve(call, cx)), Some(ErrorCode::Busy)));
}
