//! A terminal view may carry the phone's size: Herdr holds the pane's
//! terminal at it through a direct attach for exactly as long as the view
//! runs, and a failed attach still shows the screen.

use super::*;
use serde_json::json;
use wire::{TermSize, View};

fn parse(value: serde_json::Value) -> Option<ClientMessage> {
    serde_json::from_value(value).ok()
}

pub(super) fn term(size: Option<TermSize>) -> ClientMessage {
    ClientMessage::Subscribe {
        view: View::Term { pane: "p2".into() },
        since: None,
        size,
    }
}

#[test]
fn a_terminal_subscription_carries_its_size_or_none() {
    assert_eq!(
        parse(json!({
            "type": "subscribe", "view": "term", "pane": "p2",
            "size": {"cols": 46, "rows": 30},
        })),
        Some(term(Some(TermSize { cols: 46, rows: 30 })))
    );
    assert_eq!(
        parse(json!({"type": "subscribe", "view": "term", "pane": "p2"})),
        Some(term(None))
    );
    for bad in [
        json!({"cols": 46}),
        json!({"cols": -1, "rows": 30}),
        json!({"cols": 70_000, "rows": 30}),
        json!({"cols": "46", "rows": 30}),
    ] {
        let message = json!({"type": "subscribe", "view": "term", "pane": "p2", "size": bad});
        assert_eq!(parse(message), None, "{bad}");
    }
}

#[test]
fn only_a_phone_sized_terminal_is_valid() {
    for (cols, rows, valid) in [
        (20, 5, true),
        (46, 30, true),
        (400, 200, true),
        (19, 30, false),
        (401, 30, false),
        (46, 4, false),
        (46, 201, false),
        (0, 0, false),
    ] {
        assert_eq!(TermSize { cols, rows }.is_valid(), valid, "{cols}x{rows}");
    }
}

#[tokio::test]
async fn a_size_out_of_range_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::start(offline(dir.path(), layout(EndpointId::random())));
    session
        .send(term(Some(TermSize {
            cols: 2000,
            rows: 30,
        })))
        .await;
    assert_eq!(
        session.next().await,
        ServerMessage::Error {
            message: "terminal size out of range".into()
        }
    );
}

#[cfg(unix)]
pub(super) mod attached {
    use super::*;
    use herdr_client::protocol::{
        ClientMessage as Wire, MAX_FRAME_SIZE, RenderEncoding, ServerMessage as Herdr,
        read_message, write_message,
    };
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::{UnixListener, UnixStream},
    };
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

    /// What the mock Herdr's client socket received, per connection.
    pub(in super::super) type Received = UnboundedReceiver<(usize, Wire)>;

    /// A state whose Herdr API answers `pane.get` with `term_7` and reads a
    /// screen, and whose client socket takes direct attaches, or refuses
    /// them when `refuse`.
    pub(in super::super) fn with_herdr(dir: &std::path::Path, refuse: bool) -> (Shared, Received) {
        let api = UnixListener::bind(dir.join("h.sock")).unwrap();
        std::thread::spawn(move || {
            for stream in api.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let request: serde_json::Value = serde_json::from_str(&line).unwrap();
                let result = match request["method"].as_str() {
                    Some("pane.get") => json!({
                        "type": "pane_info",
                        "pane": {"pane_id": "p2", "terminal_id": "term_7"},
                    }),
                    _ => json!({"read": {"text": "$ ls"}}),
                };
                let answer = json!({"id": "remote", "result": result});
                let _ = stream.write_all(format!("{answer}\n").as_bytes());
            }
        });
        let client = UnixListener::bind(dir.join("h-client.sock")).unwrap();
        let (seen, received) = unbounded_channel();
        std::thread::spawn(move || {
            for (index, stream) in client.incoming().enumerate() {
                let Ok(stream) = stream else { return };
                let seen = seen.clone();
                std::thread::spawn(move || serve(index, stream, refuse, &seen));
            }
        });
        let hub = Hub::offline_with_api(layout(EndpointId::random()), dir.join("h-client.sock"));
        (crate::web::tests::state_with(dir, hub), received)
    }

    /// One direct client: welcome it, take or refuse its attach, then pass
    /// on everything it sends until it closes.
    fn serve(
        index: usize,
        mut stream: UnixStream,
        refuse: bool,
        seen: &UnboundedSender<(usize, Wire)>,
    ) {
        let next = |stream: &mut UnixStream| read_message::<_, Wire>(stream, MAX_FRAME_SIZE).ok();
        let Some(hello) = next(&mut stream) else {
            return;
        };
        let _ = seen.send((index, hello));
        let welcome = Herdr::Welcome {
            version: 22,
            encoding: RenderEncoding::TerminalAnsi,
            error: None,
        };
        write_message(&mut stream, &welcome, MAX_FRAME_SIZE).unwrap();
        let Some(attach) = next(&mut stream) else {
            return;
        };
        let _ = seen.send((index, attach));
        let answer = if refuse {
            Herdr::ServerShutdown {
                reason: Some("terminal term_7 already has an attached client".into()),
            }
        } else {
            Herdr::WindowTitle { title: None }
        };
        write_message(&mut stream, &answer, MAX_FRAME_SIZE).unwrap();
        while let Some(message) = next(&mut stream) {
            let _ = seen.send((index, message));
        }
    }

    pub(in super::super) async fn received(received: &mut Received) -> (usize, Wire) {
        tokio::time::timeout(Duration::from_secs(5), received.recv())
            .await
            .expect("Herdr received nothing in time")
            .expect("the mock Herdr stopped")
    }

    /// The next screen, past any other message.
    pub(in super::super) async fn screen(session: &mut Session) {
        loop {
            if let ServerMessage::TermScreen { .. } = session.next().await {
                return;
            }
        }
    }

    pub(in super::super) fn size(cols: u16, rows: u16) -> Option<TermSize> {
        Some(TermSize { cols, rows })
    }

    #[tokio::test]
    async fn a_sized_view_holds_the_terminal_until_it_is_unsubscribed() {
        let dir = tempfile::tempdir().unwrap();
        let (state, mut herdr) = with_herdr(dir.path(), false);
        let mut session = Session::start(state);
        session.send(term(size(46, 30))).await;
        let (first, hello) = received(&mut herdr).await;
        assert!(
            matches!(
                hello,
                Wire::TerminalHello {
                    cols: 46,
                    rows: 30,
                    ..
                }
            ),
            "{hello:?}"
        );
        assert_eq!(
            received(&mut herdr).await,
            (
                first,
                Wire::AttachTerminal {
                    terminal_id: "term_7".into(),
                    takeover: false,
                }
            )
        );
        screen(&mut session).await;

        // Subscribed again at a new size: the same attach resizes, and the
        // screen goes out again though it did not change.
        session.send(term(size(60, 40))).await;
        let (index, resize) = received(&mut herdr).await;
        assert_eq!(index, first, "the attach was opened again");
        assert!(
            matches!(
                resize,
                Wire::Resize {
                    cols: 60,
                    rows: 40,
                    ..
                }
            ),
            "{resize:?}"
        );
        screen(&mut session).await;

        session
            .send(ClientMessage::Unsubscribe {
                view: View::Term { pane: "p2".into() },
            })
            .await;
        assert_eq!(received(&mut herdr).await, (first, Wire::Detach));
    }

    #[tokio::test]
    async fn closing_the_socket_releases_the_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let (state, mut herdr) = with_herdr(dir.path(), false);
        let mut session = Session::start(state);
        session.send(term(size(46, 30))).await;
        received(&mut herdr).await;
        received(&mut herdr).await;
        screen(&mut session).await;
        drop(session);
        assert_eq!(received(&mut herdr).await.1, Wire::Detach);
    }

    #[tokio::test]
    async fn subscribing_again_without_a_size_releases_the_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let (state, mut herdr) = with_herdr(dir.path(), false);
        let mut session = Session::start(state);
        session.send(term(size(46, 30))).await;
        received(&mut herdr).await;
        received(&mut herdr).await;
        screen(&mut session).await;
        session.send(term(None)).await;
        assert_eq!(received(&mut herdr).await.1, Wire::Detach);
        screen(&mut session).await;
    }

    #[tokio::test]
    async fn a_refused_attach_still_shows_the_screen() {
        let dir = tempfile::tempdir().unwrap();
        let (state, mut herdr) = with_herdr(dir.path(), true);
        let mut session = Session::start(state);
        session.send(term(size(46, 30))).await;
        // Refused, then tried once more, then followed at the layout size.
        for _ in 0..2 {
            assert!(matches!(
                received(&mut herdr).await.1,
                Wire::TerminalHello { .. }
            ));
            assert!(matches!(
                received(&mut herdr).await.1,
                Wire::AttachTerminal { .. }
            ));
        }
        screen(&mut session).await;
    }
}
