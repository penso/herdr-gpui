use super::*;
use std::{
    io::Write,
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

fn wait_for_cancellation(incoming: &socket::Incoming) {
    let until = Instant::now() + Duration::from_secs(5);
    while incoming.is_live() {
        assert!(
            Instant::now() < until,
            "receiver disconnect was not observed"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[gpui::test]
fn a_cancelled_waiter_cannot_consume_the_next_send(cx: &mut gpui::TestAppContext) {
    let directory = tempfile::Builder::new()
        .prefix("hgc")
        .tempdir_in("/tmp")
        .unwrap();
    let server = socket::Server::bind(&directory.path().join("control.sock")).unwrap();
    let request = FeedbackRequest {
        pane_id: "p1".into(),
        daemon_socket: Some("/session.sock".into()),
        wait_seconds: 600,
    };
    let mut client = UnixStream::connect(server.path()).unwrap();
    writeln!(
        client,
        "{}",
        serde_json::to_string(&Request::Feedback(request.clone())).unwrap()
    )
    .unwrap();
    let incoming = server.next().unwrap();
    let mut waiters = Vec::new();
    cx.update(|cx| {
        feedback(&request, incoming, &mut waiters, cx);
        serve_waiters(&mut waiters, cx);
        assert_eq!(cx.default_global::<Feedback>().waiting().len(), 1);
    });
    drop(client);
    wait_for_cancellation(&waiters[0].incoming);
    cx.update(|cx| {
        serve_waiters(&mut waiters, cx);
        assert!(waiters.is_empty());
        assert!(cx.default_global::<Feedback>().waiting().is_empty());
        let sent = notes(
            &NotesRequest {
                caller: Caller {
                    pane_id: Some("p1".into()),
                    daemon_socket: Some("/session.sock".into()),
                    ..Default::default()
                },
                text: "next send".into(),
            },
            cx,
        );
        assert_eq!(sent, Response::NotesSent { to: NotesTo::Kept });
        let target = FeedbackKey {
            scope: Scope::local(Path::new("/session.sock")),
            pane_id: "p1".into(),
        };
        assert_eq!(
            cx.default_global::<Feedback>().take(&target).as_deref(),
            Some("next send")
        );
    });
}

#[gpui::test]
fn cancellation_while_responding_restores_the_notes(cx: &mut gpui::TestAppContext) {
    let directory = tempfile::Builder::new()
        .prefix("hgc")
        .tempdir_in("/tmp")
        .unwrap();
    let server = socket::Server::bind(&directory.path().join("control.sock")).unwrap();
    let mut client = UnixStream::connect(server.path()).unwrap();
    client.write_all(b"{\"method\":\"browser.feedback\",\"pane_id\":\"p1\",\"daemon_socket\":\"/session.sock\",\"wait_seconds\":600}\n").unwrap();
    let incoming = server.next().unwrap();
    drop(client);
    wait_for_cancellation(&incoming);
    let target = FeedbackKey {
        scope: Scope::local(Path::new("/session.sock")),
        pane_id: "p1".into(),
    };
    cx.update(|cx| {
        respond_feedback(incoming, target.clone(), Some("still unread".into()), cx);
        assert_eq!(
            cx.default_global::<Feedback>().take(&target).as_deref(),
            Some("still unread")
        );
    });
}
