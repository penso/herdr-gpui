//! The attach speaks Herdr's direct terminal handshake against a mock peer:
//! hello, attach without takeover, resize, and `Detach` on drop; refusals
//! surface as typed errors.

use super::*;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

mod scroll;

fn receive(stream: &mut Stream) -> ClientMessage {
    read_message(stream, MAX_FRAME_SIZE).unwrap()
}

fn send(stream: &mut Stream, message: ServerMessage) {
    write_message(stream, &message, MAX_FRAME_SIZE).unwrap();
}

fn welcome(version: u32, error: Option<&str>) -> ServerMessage {
    ServerMessage::Welcome {
        version,
        encoding: RenderEncoding::TerminalAnsi,
        error: error.map(str::to_owned),
    }
}

/// The client ends of `count` connected pairs, handed out in order, and
/// the peer ends a mock Herdr reads.
fn streams(count: usize) -> (impl FnMut() -> io::Result<Stream>, Vec<Stream>) {
    let mut clients = Vec::new();
    let mut peers = Vec::new();
    for _ in 0..count {
        let (client, peer) = Stream::pair().unwrap();
        clients.push(client);
        peers.push(peer);
    }
    for peer in &mut peers {
        peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        peer.set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
    }
    clients.reverse();
    let connect = move || {
        clients
            .pop()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))
    };
    (connect, peers)
}

fn hello_of(message: &ClientMessage) -> (u32, u16, u16) {
    let ClientMessage::TerminalHello {
        version,
        cols,
        rows,
        pixel_mouse,
        ..
    } = message
    else {
        panic!("not a terminal hello: {message:?}");
    };
    assert!(!pixel_mouse);
    (*version, *cols, *rows)
}

#[test]
fn it_attaches_without_takeover_resizes_and_detaches_on_drop() {
    let (connect, mut peers) = streams(1);
    let mut peer = peers.remove(0);
    let (resized, resize_seen) = mpsc::channel();
    let herdr = thread::spawn(move || {
        let hello = receive(&mut peer);
        send(&mut peer, welcome(TERMINAL_PROTOCOL_VERSION, None));
        let attach = receive(&mut peer);
        // A frame other than a refusal: the attach is taken.
        send(&mut peer, ServerMessage::WindowTitle { title: None });
        let resize = receive(&mut peer);
        resized.send(()).unwrap();
        let detach = receive(&mut peer);
        (hello, attach, resize, detach)
    });
    let attach = TerminalAttach::open_with(connect, "term_1", 46, 30).unwrap();
    assert!(attach.is_attached());
    attach.resize(60, 40).unwrap();
    resize_seen.recv_timeout(Duration::from_secs(3)).unwrap();
    drop(attach);
    let (hello, attach, resize, detach) = herdr.join().unwrap();
    assert_eq!(hello_of(&hello), (TERMINAL_PROTOCOL_VERSION, 46, 30));
    assert_eq!(
        attach,
        ClientMessage::AttachTerminal {
            terminal_id: "term_1".into(),
            takeover: false,
        }
    );
    assert!(matches!(
        resize,
        ClientMessage::Resize {
            cols: 60,
            rows: 40,
            ..
        }
    ));
    assert_eq!(detach, ClientMessage::Detach);
}

#[test]
fn a_hello_at_another_version_is_tried_again_at_herdrs() {
    let (connect, mut peers) = streams(2);
    let mut second = peers.pop().unwrap();
    let mut first = peers.pop().unwrap();
    let herdr = thread::spawn(move || {
        let refused = receive(&mut first);
        send(&mut first, welcome(23, Some("client version 22 is older")));
        let retried = receive(&mut second);
        send(&mut second, welcome(23, None));
        receive(&mut second);
        // Silence after the attach also counts as taken.
        let detach = receive(&mut second);
        (refused, retried, detach)
    });
    let attach = TerminalAttach::open_with(connect, "term_1", 80, 24).unwrap();
    drop(attach);
    let (refused, retried, detach) = herdr.join().unwrap();
    assert_eq!(hello_of(&refused).0, TERMINAL_PROTOCOL_VERSION);
    assert_eq!(hello_of(&retried), (23, 80, 24));
    assert_eq!(detach, ClientMessage::Detach);
}

#[test]
fn a_hello_refused_at_the_same_version_is_not_retried() {
    let (connect, mut peers) = streams(1);
    let mut peer = peers.remove(0);
    let herdr = thread::spawn(move || {
        receive(&mut peer);
        send(
            &mut peer,
            welcome(TERMINAL_PROTOCOL_VERSION, Some("no terminals")),
        );
    });
    let error = TerminalAttach::open_with(connect, "term_1", 80, 24).unwrap_err();
    herdr.join().unwrap();
    assert!(
        matches!(&error, Error::Rejected { version, reason }
            if *version == TERMINAL_PROTOCOL_VERSION && reason == "no terminals"),
        "{error:?}"
    );
}

#[test]
fn a_terminal_another_client_holds_is_refused() {
    let (connect, mut peers) = streams(1);
    let mut peer = peers.remove(0);
    let herdr = thread::spawn(move || {
        receive(&mut peer);
        send(&mut peer, welcome(TERMINAL_PROTOCOL_VERSION, None));
        receive(&mut peer);
        send(
            &mut peer,
            ServerMessage::ServerShutdown {
                reason: Some("terminal term_1 already has an attached client".into()),
            },
        );
    });
    let error = TerminalAttach::open_with(connect, "term_1", 80, 24).unwrap_err();
    herdr.join().unwrap();
    assert!(
        matches!(&error, Error::Refused(Some(reason)) if reason.contains("already")),
        "{error:?}"
    );
    let root = crate::Error::from(error);
    assert!(matches!(
        root,
        crate::Error::TerminalAttach(Error::Refused(_))
    ));
}

#[test]
fn a_connection_closed_after_the_attach_is_a_refusal() {
    let (connect, mut peers) = streams(1);
    let mut peer = peers.remove(0);
    let herdr = thread::spawn(move || {
        receive(&mut peer);
        send(&mut peer, welcome(TERMINAL_PROTOCOL_VERSION, None));
        receive(&mut peer);
    });
    let result = TerminalAttach::open_with(connect, "term_1", 80, 24);
    herdr.join().unwrap();
    assert!(matches!(result, Err(Error::Refused(None))), "{result:?}");
}

#[test]
fn an_attach_herdr_closes_ends_and_refuses_resizes() {
    let (connect, mut peers) = streams(1);
    let mut peer = peers.remove(0);
    let (attached, closing) = mpsc::channel::<()>();
    let herdr = thread::spawn(move || {
        receive(&mut peer);
        send(&mut peer, welcome(TERMINAL_PROTOCOL_VERSION, None));
        receive(&mut peer);
        send(&mut peer, ServerMessage::WindowTitle { title: None });
        closing.recv().unwrap();
        drop(peer);
    });
    let attach = TerminalAttach::open_with(connect, "term_1", 80, 24).unwrap();
    attached.send(()).unwrap();
    herdr.join().unwrap();
    let due = Instant::now() + Duration::from_secs(3);
    while attach.is_attached() {
        assert!(Instant::now() < due, "the attach never noticed the close");
        thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(attach.resize(80, 24), Err(Error::Ended)));
}

#[test]
fn a_zero_size_is_refused_before_connecting() {
    let mut connected = false;
    let connect = || {
        connected = true;
        Err(io::Error::from(io::ErrorKind::NotConnected))
    };
    assert!(matches!(
        TerminalAttach::open_with(connect, "term_1", 0, 24),
        Err(Error::Size)
    ));
    assert!(!connected);
}
