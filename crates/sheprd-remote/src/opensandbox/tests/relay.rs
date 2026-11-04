//! The relay against a scripted execd stream on loopback.

use super::*;
use crate::opensandbox::relay::{Notice, notice, relay};
use std::{io::Cursor, net::TcpListener, sync::mpsc, thread, time::Duration};
use tungstenite::Message;

#[test]
fn execd_notices_end_or_fail_the_command() {
    assert_eq!(notice(r#"{"type":"exit","exit_code":3}"#), Notice::Exit(3));
    assert_eq!(
        notice(r#"{"type":"exit","exit_code":300}"#),
        Notice::Exit(255)
    );
    assert_eq!(
        notice(r#"{"type":"error","code":"START_FAILED","error":"no\nshell"}"#),
        Notice::Failed("noshell".into())
    );
    assert_eq!(
        notice(r#"{"type":"connected","mode":"pipe","role":"holder"}"#),
        Notice::Other
    );
}

fn serve(replies: Vec<Message>) -> (String, mpsc::Receiver<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sent, received) = mpsc::channel();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        socket
            .send(Message::Text(
                r#"{"type":"connected","mode":"pipe","role":"holder"}"#.into(),
            ))
            .unwrap();
        let mut frames = Vec::new();
        while let Message::Binary(frame) = socket.read().unwrap() {
            let done = frame[..] == b"\x000\n"[..];
            frames.push(frame.to_vec());
            if done {
                break;
            }
        }
        for reply in replies {
            socket.send(reply).unwrap();
        }
        let _ = sent.send(frames);
        thread::sleep(Duration::from_millis(200));
    });
    (format!("ws://{address}/pty/s/ws?pty=0"), received)
}

#[test]
fn the_relay_frames_input_and_splits_output_by_stream() {
    let (url, frames) = serve(vec![
        Message::Binary(vec![0x01, 0, 255].into()),
        Message::Binary(vec![0x02, b'e'].into()),
        Message::Text(r#"{"type":"exit","exit_code":5}"#.into()),
    ]);
    let mut socket = crate::cloud::socket::open(&url::Url::parse(&url).unwrap(), &[]).unwrap();
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = relay(
        &mut socket,
        crate::cloud::socket::input(Cursor::new(b"hi".to_vec())),
        &mut stdout,
        &mut stderr,
    )
    .unwrap();
    assert_eq!(code, 5);
    assert_eq!((stdout, stderr), (vec![0, 255], b"e".to_vec()));
    let frames = frames.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(frames, [b"\x002\nhi".to_vec(), b"\x000\n".to_vec()]);
}

#[test]
fn an_execd_error_fails_the_relay() {
    let (url, _) = serve(vec![Message::Text(
        r#"{"type":"error","code":"START_FAILED","error":"bad"}"#.into(),
    )]);
    let mut socket = crate::cloud::socket::open(&url::Url::parse(&url).unwrap(), &[]).unwrap();
    let error = relay(
        &mut socket,
        crate::cloud::socket::input(Cursor::new(Vec::new())),
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .unwrap_err();
    assert!(matches!(error, Error::Shell(reason) if reason == "bad"));
}
