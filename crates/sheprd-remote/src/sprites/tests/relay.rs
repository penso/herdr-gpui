//! The relay against a scripted exec stream on loopback.

use super::*;
use std::{io::Cursor, net::TcpListener, sync::mpsc, thread, time::Duration};
use tungstenite::Message;

pub(super) use super::super::relay::{HELPER, run_helper};
use super::super::relay::{open, relay};
use crate::cloud::socket::{Stream, input};

/// Serve one exec stream: record what the client sends until its stdin
/// ends, then send `replies`.
fn serve(replies: Vec<Message>) -> (String, mpsc::Receiver<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sent, received) = mpsc::channel();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let mut frames = Vec::new();
        loop {
            if let Message::Binary(frame) = socket.read().unwrap() {
                let done = frame.first() == Some(&0x04);
                frames.push(frame.to_vec());
                if done {
                    break;
                }
            }
        }
        for reply in replies {
            socket.send(reply).unwrap();
        }
        let _ = sent.send(frames);
        thread::sleep(Duration::from_millis(200));
    });
    (format!("ws://{address}/v1/sprites/s/exec"), received)
}

fn connect(url: &str) -> tungstenite::WebSocket<Stream> {
    let url = url::Url::parse(url).unwrap();
    open(&url, &"token".to_owned().into()).unwrap()
}

#[test]
fn stdin_goes_up_tagged_and_output_comes_back_split_by_stream() {
    let (url, frames) = serve(vec![
        Message::Text(r#"{"type":"session_info","session_id":"1"}"#.into()),
        Message::Binary(vec![0x01, 0, 159, 146, 150].into()),
        Message::Binary(vec![0x02, b'w', b'a', b'r', b'n'].into()),
        Message::Binary(vec![0x01, b'!'].into()),
        Message::Binary(vec![0x03, 7].into()),
    ]);
    let mut socket = connect(&url);
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = relay(
        &mut socket,
        input(Cursor::new(b"hello\x00binary".to_vec())),
        &mut stdout,
        &mut stderr,
    )
    .unwrap();
    assert_eq!(code, 7);
    // Bytes that are not UTF-8 pass unchanged.
    assert_eq!(stdout, [0, 159, 146, 150, b'!']);
    assert_eq!(stderr, b"warn");
    let frames = frames.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut sent = Vec::new();
    for frame in &frames[..frames.len() - 1] {
        assert_eq!(frame[0], 0x00, "stdin frames carry its tag");
        sent.extend_from_slice(&frame[1..]);
    }
    assert_eq!(sent, b"hello\x00binary");
    assert_eq!(
        frames.last().unwrap(),
        &[0x04],
        "stdin ends with its EOF frame"
    );
}

#[test]
fn an_exit_message_ends_the_command_too() {
    let (url, _) = serve(vec![Message::Text(
        r#"{"type":"exit","exit_code":3}"#.into(),
    )]);
    let mut socket = connect(&url);
    let code = relay(
        &mut socket,
        input(Cursor::new(Vec::new())),
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(code, 3);
}

#[test]
fn a_stream_that_closes_without_an_exit_is_a_failed_connection() {
    let (url, _) = serve(vec![Message::Close(None)]);
    let mut socket = connect(&url);
    let error = relay(
        &mut socket,
        input(Cursor::new(Vec::new())),
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        Error::Socket(crate::cloud::socket::Error::Closed)
    ));
}
