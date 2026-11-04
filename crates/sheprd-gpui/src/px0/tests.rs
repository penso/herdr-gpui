#![allow(clippy::unwrap_used)]
use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::Duration,
};

pub(super) struct HttpServer {
    pub(super) url: WebUrl,
    pub(super) requests: mpsc::Receiver<String>,
    stop: mpsc::Sender<()>,
    thread: Option<thread::JoinHandle<()>>,
}

impl HttpServer {
    pub(super) fn new(status: u16, body: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = WebUrl::try_from(format!("http://{}/", listener.local_addr().unwrap()).as_str())
            .unwrap();
        let response = format!(
            "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (send, requests) = mpsc::channel();
        let (stop, stopped) = mpsc::channel();
        let thread = thread::spawn(move || {
            loop {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        // Accepted sockets can inherit O_NONBLOCK on macOS;
                        // read the full request rather than answering before it arrives.
                        socket.set_nonblocking(false).unwrap();
                        socket
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut request = Vec::new();
                        let mut byte = [0];
                        while request.len() < 8192 && !request.ends_with(b"\r\n\r\n") {
                            if socket.read(&mut byte).unwrap_or(0) == 0 {
                                break;
                            }
                            request.push(byte[0]);
                        }
                        let _ = send.send(String::from_utf8_lossy(&request).into_owned());
                        let _ = socket.write_all(response.as_bytes());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) => panic!("{error}"),
                }
                if stopped.recv_timeout(Duration::from_millis(5)).is_ok() {
                    return;
                }
            }
        });
        Self {
            url,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

#[test]
fn readiness_requires_a_bounded_workbench_response() {
    let server = HttpServer::new(200, r#"{"workbench":true,"version":"0.1.17"}"#);
    let base = WebUrl::try_from(format!("{}reviews/", server.url.as_str()).as_str()).unwrap();
    server::probe(&base).unwrap();
    assert!(
        server
            .requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .starts_with("GET /reviews/api/wb/ping ")
    );
    for (status, body) in [
        (302, "{}".to_owned()),
        (200, r#"{"workbench":false,"version":"1"}"#.to_owned()),
        (200, "x".repeat(8193)),
    ] {
        let other = HttpServer::new(status, &body);
        let result = server::probe(&other.url);
        assert!(
            matches!(result, Err(Error::NotWorkbench)),
            "status {status}, body bytes {}: {result:?}",
            body.len()
        );
    }
    let other = HttpServer::new(200, "{");
    let error = server::probe(&other.url).unwrap_err();
    assert!(matches!(error, Error::Json(_)));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn discovery_rejects_stale_malformed_and_nonlocal_records() {
    let temp = tempfile::tempdir().unwrap();
    let lock = temp.path().join("workbench.lock");
    assert!(server::discover(&lock).unwrap().is_none());
    for data in [
        "{".to_owned(),
        "x".repeat(8193),
        r#"{"pid":7,"url":"http://example.com/"}"#.into(),
        r#"{"pid":0,"url":"http://localhost/"}"#.into(),
    ] {
        std::fs::write(&lock, data).unwrap();
        assert!(server::discover(&lock).unwrap().is_none());
    }
    std::fs::write(&lock, r#"{"pid":7,"url":"http://127.0.0.1:54321/"}"#).unwrap();
    let found = server::discover(&lock).unwrap().unwrap();
    assert_eq!(found.pid, 7);
    assert_eq!(found.url.as_str(), "http://127.0.0.1:54321/");
}

#[test]
fn only_local_workbench_addresses_are_accepted() {
    for address in [
        "http://127.0.0.1:7777/",
        "http://localhost:7777/reviews/",
        "http://[::1]:7777/",
    ] {
        assert!(server::base(&WebUrl::try_from(address).unwrap()).is_ok());
    }
    for address in [
        "http://0.0.0.0:7777/",
        "http://example.com/",
        "http://127.0.0.1/?open=other",
        "http://127.0.0.1/#x",
        "https://localhost/",
    ] {
        assert!(matches!(
            server::base(&WebUrl::try_from(address).unwrap()),
            Err(Error::Address)
        ));
    }
}

#[test]
fn workspace_paths_are_encoded_and_proxy_prefix_is_preserved() {
    let base = WebUrl::try_from("http://localhost:7777/reviews").unwrap();
    let root = "/work/tree & review/#main";
    let opened = workspace_url(&base, root).unwrap();
    let parsed = url::Url::parse(opened.as_str()).unwrap();
    assert_eq!(parsed.path(), "/reviews/");
    assert_eq!(
        parsed.query_pairs().collect::<Vec<_>>(),
        [("open".into(), root.into())]
    );
}

#[gpui::test]
fn installing_a_launcher_does_not_start_px0(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        Launcher::initialize(cx);
        assert!(matches!(Launcher::state(cx), State::Idle));
        assert!(cx.global::<Launcher>().worker.is_none());
        Launcher::apply_config(cx, Config::default());
        assert!(cx.global::<Launcher>().worker.is_none());
    });
}

#[gpui::test]
fn a_changed_address_retires_the_previous_result(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let old = Config {
            url: Some(WebUrl::try_from("http://localhost:7777/").unwrap()),
        };
        let next = Config {
            url: Some(WebUrl::try_from("http://localhost:8888/").unwrap()),
        };
        cx.set_global(Launcher {
            config: Some(old.clone()),
            started: true,
            worker: None,
            state: State::Failed(std::sync::Arc::new(Error::Timeout)),
            ..Default::default()
        });
        Launcher::apply_config(cx, old);
        assert!(matches!(Launcher::state(cx), State::Failed(_)));
        Launcher::apply_config(cx, next.clone());
        assert!(matches!(Launcher::state(cx), State::Idle));
        assert!(cx.global::<Launcher>().worker.is_none());
        assert_eq!(cx.global::<Launcher>().config.as_ref(), Some(&next));
    });
}
