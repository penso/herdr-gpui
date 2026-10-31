//! A phone swipes a terminal it follows at its own size: the swipe goes
//! to Herdr as a wheel scroll over the held attach, within bounds, and a
//! terminal no attach holds says once that it cannot scroll.

use super::*;
use serde_json::json;

fn parse(value: serde_json::Value) -> Option<ClientMessage> {
    serde_json::from_value(value).ok()
}

fn scroll(lines: i32) -> ClientMessage {
    ClientMessage::TermScroll {
        pane: "p2".into(),
        lines,
    }
}

#[test]
fn a_scroll_carries_signed_lines() {
    for lines in [3, -3, 50, -400] {
        assert_eq!(
            parse(json!({"type": "term_scroll", "pane": "p2", "lines": lines})),
            Some(scroll(lines))
        );
    }
    for bad in [
        json!({"type": "term_scroll", "pane": "p2"}),
        json!({"type": "term_scroll", "lines": 3}),
        json!({"type": "term_scroll", "pane": "p2", "lines": "3"}),
        json!({"type": "term_scroll", "pane": "p2", "lines": 1.5}),
        json!({"type": "term_scroll", "pane": "p2", "lines": 10_000_000_000_i64}),
    ] {
        assert_eq!(parse(bad.clone()), None, "{bad}");
    }
}

#[tokio::test]
async fn a_scroll_of_no_lines_or_of_an_unfollowed_terminal_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::start(offline(dir.path(), layout(EndpointId::random())));
    for (message, expected) in [
        (scroll(0), "scroll out of range"),
        (scroll(3), "this terminal cannot scroll from the phone"),
    ] {
        session.send(message).await;
        assert_eq!(
            session.next().await,
            ServerMessage::Error {
                message: expected.into()
            }
        );
    }
}

#[cfg(unix)]
mod attached {
    use super::super::term_size::{
        attached::{received, screen, size, with_herdr},
        term,
    };
    use super::*;
    use herdr_client::protocol::{
        AttachScrollDirection, AttachScrollSource, ClientMessage as Wire,
    };

    fn wheel(direction: AttachScrollDirection, lines: u16) -> Wire {
        Wire::AttachScroll {
            source: AttachScrollSource::Wheel,
            direction,
            lines,
            column: Some(23),
            row: Some(15),
            modifiers: 0,
        }
    }

    #[tokio::test]
    async fn a_swipe_scrolls_the_held_terminal_at_its_middle() {
        let dir = tempfile::tempdir().unwrap();
        let (state, mut herdr) = with_herdr(dir.path(), false);
        let mut session = Session::start(state);
        session.send(term(size(46, 30))).await;
        let (attach, _) = received(&mut herdr).await;
        received(&mut herdr).await;
        screen(&mut session).await;
        session.send(scroll(3)).await;
        assert_eq!(
            received(&mut herdr).await,
            (attach, wheel(AttachScrollDirection::Up, 3))
        );
        // Past the bound, a swipe moves the most one may.
        session.send(scroll(-80)).await;
        assert_eq!(
            received(&mut herdr).await,
            (attach, wheel(AttachScrollDirection::Down, 50))
        );
    }

    #[tokio::test]
    async fn a_terminal_at_the_layout_size_says_once_it_cannot_scroll() {
        let dir = tempfile::tempdir().unwrap();
        let (state, mut herdr) = with_herdr(dir.path(), false);
        let mut session = Session::start(state);
        session.send(term(None)).await;
        screen(&mut session).await;
        session.send(scroll(3)).await;
        session.send(scroll(3)).await;
        assert_eq!(
            session.next().await,
            ServerMessage::Error {
                message: "this terminal cannot scroll from the phone".into()
            }
        );
        let again =
            tokio::time::timeout(Duration::from_millis(500), session.from_live.recv()).await;
        assert!(again.is_err(), "told again: {again:?}");
        assert!(herdr.try_recv().is_err(), "Herdr was reached");
    }
}
