use super::*;

fn feedback_request() -> Request {
    Request::Feedback(FeedbackRequest {
        pane_id: "p1".into(),
        daemon_socket: Some("/session.sock".into()),
        wait_seconds: 600,
    })
}

#[test]
fn disconnected_receivers_cancel_without_waiting_for_the_deadline() {
    let (server, client) = UnixStream::pair().unwrap();
    let (sender, events) = mpsc::sync_channel(QUEUE);
    let (finished, done) = mpsc::channel();
    let worker = thread::spawn(move || {
        serve(server, &sender);
        finished.send(()).unwrap();
    });
    write_line(&client, &feedback_request()).unwrap();
    let Event::Request(incoming) = events.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("expected request");
    };
    assert!(incoming.is_live());
    drop(client);
    done.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.join().unwrap();
    assert!(!incoming.is_live());
    let response = Response::Feedback {
        text: Some("keep these notes".into()),
    };
    assert_eq!(incoming.try_respond(response.clone()), Err(response));
}

#[test]
fn failed_socket_writes_return_notes_with_their_original_session() {
    let (server, client) = UnixStream::pair().unwrap();
    let (sender, events) = mpsc::sync_channel(QUEUE);
    drop(client);
    finish_response(
        &server,
        &sender,
        Some(feedback_request()),
        Response::Feedback {
            text: Some("unread notes".into()),
        },
    );
    let Event::Undelivered { request, text } = events.recv_timeout(Duration::from_secs(5)).unwrap()
    else {
        panic!("expected returned notes");
    };
    assert_eq!(Request::Feedback(request), feedback_request());
    assert_eq!(text, "unread notes");
    assert!(events.try_recv().is_err());
}
