use super::*;

mod cancellation;

fn queued_feedback(
    server: &socket::Server,
    request: &FeedbackRequest,
) -> (
    socket::Incoming,
    std::thread::JoinHandle<crate::Result<Response>>,
) {
    let path = server.path().to_owned();
    let request = request.clone();
    let caller = std::thread::spawn(move || socket::call(&path, &Request::Feedback(request)));
    (server.next().unwrap(), caller)
}

#[gpui::test]
fn scoped_feedback_requests_keep_same_named_waiters_separate(cx: &mut gpui::TestAppContext) {
    let directory = tempfile::Builder::new()
        .prefix("hgc")
        .tempdir_in("/tmp")
        .unwrap();
    let server = socket::Server::bind(&directory.path().join("control.sock")).unwrap();
    let first = FeedbackRequest {
        pane_id: "p1".into(),
        daemon_socket: Some("/first.sock".into()),
        wait_seconds: 30,
    };
    let other = FeedbackRequest {
        daemon_socket: Some("/other.sock".into()),
        ..first.clone()
    };
    let (first_incoming, first_call) = queued_feedback(&server, &first);
    let (other_incoming, other_call) = queued_feedback(&server, &other);
    let legacy = FeedbackRequest {
        daemon_socket: None,
        ..first.clone()
    };
    let (legacy_incoming, legacy_call) = queued_feedback(&server, &legacy);
    cx.update(|cx| {
        let mut waiters = Vec::new();
        feedback(&first, first_incoming, &mut waiters, cx);
        feedback(&other, other_incoming, &mut waiters, cx);
        feedback(&legacy, legacy_incoming, &mut waiters, cx);
        assert_eq!(waiters.len(), 2);
        serve_waiters(&mut waiters, cx);
        let request = |daemon: &str, text: &str| NotesRequest {
            caller: Caller {
                pane_id: Some("p1".into()),
                daemon_socket: Some(daemon.into()),
                ..Default::default()
            },
            text: text.into(),
        };
        assert_eq!(
            notes(&request("/first.sock", "first only"), cx),
            Response::NotesSent { to: NotesTo::Agent }
        );
        serve_waiters(&mut waiters, cx);
        assert_eq!(waiters.len(), 1);
        assert_eq!(
            waiters[0].target.scope,
            Scope::local(Path::new("/other.sock"))
        );
        assert_eq!(
            notes(&request("/other.sock", "other only"), cx),
            Response::NotesSent { to: NotesTo::Agent }
        );
        serve_waiters(&mut waiters, cx);
        assert!(waiters.is_empty());
    });
    assert_eq!(
        first_call.join().unwrap().unwrap(),
        Response::Feedback {
            text: Some("first only".into())
        }
    );
    assert_eq!(
        other_call.join().unwrap().unwrap(),
        Response::Feedback {
            text: Some("other only".into())
        }
    );
    assert!(matches!(
        legacy_call.join().unwrap().unwrap(),
        Response::Error {
            code: ErrorCode::InvalidRequest,
            ..
        }
    ));
}

#[gpui::test]
fn notes_without_a_matching_window_keep_the_callers_daemon(cx: &mut gpui::TestAppContext) {
    let first = FeedbackKey {
        scope: Scope::local(Path::new("/first.sock")),
        pane_id: "p1".into(),
    };
    let other = FeedbackKey {
        scope: Scope::local(Path::new("/other.sock")),
        pane_id: "p1".into(),
    };
    cx.update(|cx| {
        cx.default_global::<Feedback>()
            .set_waiting(vec![other.clone()]);
        let request = NotesRequest {
            caller: Caller {
                pane_id: Some("p1".into()),
                daemon_socket: Some("/first.sock".into()),
                ..Default::default()
            },
            text: "first session only".into(),
        };
        assert_eq!(
            notes(&request, cx),
            Response::NotesSent { to: NotesTo::Kept }
        );
        assert_eq!(cx.default_global::<Feedback>().take(&other), None);
        assert_eq!(
            cx.default_global::<Feedback>().take(&first).as_deref(),
            Some("first session only")
        );
        let unscoped = NotesRequest {
            caller: Caller {
                daemon_socket: None,
                ..request.caller
            },
            text: request.text,
        };
        assert!(matches!(
            notes(&unscoped, cx),
            Response::Error {
                code: ErrorCode::InvalidRequest,
                ..
            }
        ));
    });
}
