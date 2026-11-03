use super::*;

#[gpui::test]
fn a_caller_naming_its_daemon_never_reaches_another_sessions_workspace(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        // The window shows a w3, but on another daemon than the caller's:
        // a same-named workspace of another session is never acted on.
        let elsewhere = Caller {
            daemon_socket: Some("/tmp/another-session.sock".into()),
            ..caller(Some("w3"))
        };
        let refused = Place::of(view, &elsewhere).unwrap_err();
        assert_eq!(error_code(&refused), Some(ErrorCode::WorkspaceNotFound));
        // Without a daemon, any endpoint showing the workspace still serves.
        assert_eq!(
            Place::of(view, &caller(Some("w3"))).unwrap().workspace,
            "w3"
        );
    });
}

#[test]
fn a_page_request_succeeds_only_when_a_page_opened() {
    let done = || "Opened port 3000".to_owned();
    assert!(matches!(
        page_answer(Some(PageOpened::Tab), done),
        Response::Done { .. }
    ));
    assert!(matches!(
        page_answer(Some(PageOpened::SystemBrowser), done),
        Response::Done { .. }
    ));
    assert_eq!(
        error_code(&page_answer(Some(PageOpened::Full), done)),
        Some(ErrorCode::TabLimit)
    );
    assert_eq!(
        error_code(&page_answer(Some(PageOpened::Unreachable), done)),
        Some(ErrorCode::Failed)
    );
    let Response::Done { message } = page_answer(None, done) else {
        panic!("a tunnel still connecting is not a failure");
    };
    assert!(message.contains("once its tunnel connects"), "{message}");
}

#[gpui::test]
fn a_caller_whose_workspace_no_window_shows_gets_no_window(cx: &mut TestAppContext) {
    let (_view, cx) = window(cx);
    cx.cx.update(|cx| {
        // A pane of a session the app does not show: the frontmost window
        // belongs to another session, and must not be acted on.
        let refused = window_for(&caller(Some("w_other_session")), cx).unwrap_err();
        assert_eq!(error_code(&refused), Some(ErrorCode::WorkspaceNotFound));
        assert!(window_for(&caller(Some("w3")), cx).is_ok());
        // Outside any workspace, the frontmost window serves.
        assert!(window_for(&caller(None), cx).is_ok());
    });
}

#[gpui::test]
fn a_window_of_the_callers_own_daemon_serves_even_listing_nothing(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let daemon = cx.cx.update(|cx| {
        view.read(cx).endpoints[0]
            .connection
            .target
            .socket_path()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    });
    cx.cx.update(|cx| {
        // Detached, the window lists no workspace of the caller's, but it
        // is the caller's daemon's window: `window reconnect` reaches it.
        let own = Caller {
            daemon_socket: Some(daemon),
            ..caller(Some("w_not_listed"))
        };
        assert!(window_for(&own, cx).is_ok());
    });
}
