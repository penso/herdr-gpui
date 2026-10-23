#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::super::protocol::{
    BrowserOpen, Caller, FeedbackRequest, MAX_WAIT_SECONDS, OpenedIn, Page,
};
use super::*;

mod cancellation;
mod framing;
#[cfg(windows)]
mod pipe;
#[cfg(unix)]
mod unix;

fn open(url: &str) -> Request {
    Request::Open(BrowserOpen {
        page: Page::Url(url.into()),
        caller: Caller::default(),
        focus: true,
        placement: crate::cli::control::Placement::Tab,
    })
}

/// A private address, removed with the guard where it is a file.
pub(crate) struct Address {
    pub path: PathBuf,
    #[cfg(unix)]
    _dir: tempfile::TempDir,
}

/// A fresh address no other test uses.
#[cfg(unix)]
pub(crate) fn address() -> Address {
    // Short: socket paths are limited to about 100 bytes.
    let dir = tempfile::Builder::new()
        .prefix("hgc")
        .tempdir_in("/tmp")
        .unwrap();
    Address {
        path: dir.path().join("control.sock"),
        _dir: dir,
    }
}

/// A fresh address no other test uses.
#[cfg(windows)]
pub(crate) fn address() -> Address {
    use std::sync::atomic::AtomicU64;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    Address {
        path: PathBuf::from(format!(
            r"\\.\pipe\herdr-gpui-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )),
    }
}

/// Both ends of one connection: the server's, timeouts set, and a caller's.
fn pair() -> (Address, Connection, Connection) {
    let address = address();
    let listener = transport::Listener::bind(&address.path).unwrap();
    let client = transport::connect(&address.path, IO_TIMEOUT, IO_TIMEOUT).unwrap();
    let mut server = listener.accept().unwrap();
    server.prepare(IO_TIMEOUT).unwrap();
    (address, server, client)
}

#[test]
fn a_request_reaches_the_ui_and_its_answer_reaches_the_caller() {
    let address = address();
    let server = Server::bind(&address.path).unwrap();
    let caller = thread::spawn({
        let path = address.path.clone();
        move || call(&path, &open("https://a.test/"))
    });
    let incoming = server.next().unwrap();
    assert_eq!(incoming.request, open("https://a.test/"));
    let expected = Response::Opened {
        opened_in: OpenedIn::Tab,
        workspace_id: Some("w_1".into()),
        placement: crate::cli::control::Placement::Beside,
    };
    incoming.respond(expected.clone());
    assert_eq!(caller.join().unwrap().unwrap(), expected);
}

#[test]
fn malformed_and_oversized_requests_are_answered_without_the_ui() {
    let address = address();
    let server = Server::bind(&address.path).unwrap();
    // Exactly one byte over, so the server reads all of it and closing
    // never discards unread input, which some kernels answer with a reset.
    for request in [b"not json\n".to_vec(), vec![b'x'; MAX_MESSAGE + 1]] {
        let stream = transport::connect(&address.path, IO_TIMEOUT, IO_TIMEOUT).unwrap();
        (&stream).write_all(&request).unwrap();
        let response: Response = serde_json::from_slice(&read_line(&stream).unwrap()).unwrap();
        assert!(
            matches!(
                response,
                Response::Error {
                    code: ErrorCode::InvalidRequest,
                    ..
                }
            ),
            "{response:?}"
        );
    }
    assert!(server.drain().next().is_none());
}

#[test]
fn a_live_server_keeps_its_address() {
    let address = address();
    let _server = Server::bind(&address.path).unwrap();
    assert!(matches!(
        Server::bind(&address.path),
        Err(crate::Error::Control(
            crate::control::Error::SocketInUse { .. }
        ))
    ));
}

#[test]
fn a_waiting_caller_does_not_hold_up_the_next_one() {
    let address = address();
    let server = Server::bind(&address.path).unwrap();
    let waiting = thread::spawn({
        let path = address.path.clone();
        move || {
            call(
                &path,
                &Request::Feedback(FeedbackRequest {
                    pane_id: "w_1:p1".into(),
                    daemon_socket: Some("/tmp/daemon.sock".into()),
                    wait_seconds: 30,
                }),
            )
        }
    });
    let parked = server.next().unwrap();
    assert!(matches!(parked.request, Request::Feedback(_)));
    // While the first caller waits for the user, another is answered.
    let next = thread::spawn({
        let path = address.path.clone();
        move || call(&path, &open("https://a.test/"))
    });
    server
        .next()
        .unwrap()
        .respond(Response::Reloaded { tabs: 0 });
    assert_eq!(
        next.join().unwrap().unwrap(),
        Response::Reloaded { tabs: 0 }
    );
    parked.respond(Response::Feedback { text: None });
    assert_eq!(
        waiting.join().unwrap().unwrap(),
        Response::Feedback { text: None }
    );
}

#[test]
fn feedback_waits_are_bounded() {
    let wait = |wait_seconds| {
        answer_timeout(&Request::Feedback(FeedbackRequest {
            pane_id: "p".into(),
            daemon_socket: Some("/tmp/daemon.sock".into()),
            wait_seconds,
        }))
    };
    assert_eq!(wait(5), ANSWER_TIMEOUT + Duration::from_secs(5));
    assert_eq!(
        wait(u64::MAX),
        ANSWER_TIMEOUT + Duration::from_secs(MAX_WAIT_SECONDS)
    );
    assert_eq!(answer_timeout(&open("https://a.test/")), ANSWER_TIMEOUT);
}

#[test]
fn an_answer_over_the_bound_is_refused_and_its_notes_kept() {
    let address = address();
    let server = Server::bind(&address.path).unwrap();
    let request = FeedbackRequest {
        pane_id: "w_1:p1".into(),
        daemon_socket: Some("/tmp/daemon.sock".into()),
        wait_seconds: 0,
    };
    let caller = thread::spawn({
        let path = address.path.clone();
        let request = request.clone();
        move || call(&path, &Request::Feedback(request))
    });
    let notes = "n".repeat(MAX_MESSAGE);
    server.next().unwrap().respond(Response::Feedback {
        text: Some(notes.clone()),
    });
    assert!(matches!(
        caller.join().unwrap().unwrap(),
        Response::Error {
            code: ErrorCode::Failed,
            ..
        }
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(Event::Undelivered { text, .. }) = server.drain().next() {
            assert_eq!(text, notes, "the notes are kept again");
            break;
        }
        assert!(Instant::now() < deadline, "the notes were lost");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_missing_server_is_reported_as_unavailable() {
    let address = address();
    assert!(matches!(
        call(&address.path, &open("https://a.test/")),
        Err(crate::Error::Control(
            crate::control::Error::Unavailable { .. }
        ))
    ));
}
