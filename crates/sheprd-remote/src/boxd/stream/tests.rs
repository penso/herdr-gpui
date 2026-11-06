#![allow(clippy::unwrap_used)]
use super::*;
use crate::boxd::wire::{Message, frame};

/// What boxd sent for `cat; echo done-$?` with stdin `hello\0\xff\n`, as
/// captured from the live service, head shortened.
const LIVE: &[u8] = b"HTTP/1.1 200 OK\r\ncontent-type: application/grpc-web+proto\r\ntransfer-encoding: chunked\r\n\r\nF\r\n\x00\x00\x00\x00\n\n\x08hello\x00\xff\n\r\nE\r\n\x00\x00\x00\x00\t\n\x07done-0\n\r\n5\r\n\x00\x00\x00\x00\x00\r\n14\r\n\x80\x00\x00\x00\x0fgrpc-status:0\r\n\r\n0\r\n\r\n";

fn live_events() -> Vec<Event> {
    vec![
        Event::Stdout(b"hello\x00\xff\n".to_vec()),
        Event::Stdout(b"done-0\n".to_vec()),
        Event::End(Trailers {
            status: Some(0),
            message: None,
        }),
    ]
}

#[test]
fn a_live_reply_decodes_whole() {
    let mut decoder = Decoder::default();
    assert_eq!(decoder.push(LIVE).unwrap(), live_events());
    assert!(decoder.done());
}

#[test]
fn a_reply_in_single_bytes_decodes_the_same() {
    let mut decoder = Decoder::default();
    let mut events = Vec::new();
    for byte in LIVE {
        events.extend(decoder.push(std::slice::from_ref(byte)).unwrap());
    }
    assert_eq!(events, live_events());
}

#[test]
fn a_frame_split_across_chunks_waits_for_its_end() {
    let message = Message::default()
        .bytes(1, b"warn")
        .bool(10, true)
        .into_bytes();
    let mut exit = Message::default().into_bytes();
    exit.extend_from_slice(&[0x30, 7]);
    let mut body = frame(&message);
    body.extend(frame(&exit));
    let (first, second) = body.split_at(3);
    let mut reply = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n".to_vec();
    for part in [first, second] {
        reply.extend(format!("{:x}\r\n", part.len()).bytes());
        reply.extend_from_slice(part);
        reply.extend_from_slice(b"\r\n");
    }
    let mut decoder = Decoder::default();
    assert_eq!(
        decoder.push(&reply).unwrap(),
        [Event::Stderr(b"warn".to_vec()), Event::Exit(7)]
    );
    assert!(!decoder.done(), "no status yet");
}

#[test]
fn a_refusal_in_the_headers_ends_the_call() {
    let mut decoder = Decoder::default();
    let events = decoder
        .push(b"HTTP/1.1 200 OK\r\ngrpc-status: 16\r\ngrpc-message: API%20key%20not%20recognized\r\ncontent-length: 0\r\n\r\n")
        .unwrap();
    assert_eq!(
        events,
        [Event::End(Trailers {
            status: Some(16),
            message: Some("API key not recognized".into()),
        })]
    );
    assert!(decoder.done());
}

#[test]
fn an_http_failure_without_grpc_is_its_own_status() {
    let error = Decoder::default()
        .push(b"HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\n\r\n")
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Status(Status {
            code: Code::Http(502),
            ..
        })
    ));
    assert!(matches!(
        Decoder::default().push(b"HTTP/1.1 401 No\r\n\r\n"),
        Err(Error::Authentication)
    ));
}

#[test]
fn broken_chunking_is_refused() {
    let head = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n";
    for bad in [&b"zz\r\n"[..], &b"1\r\nabc"[..]] {
        let mut reply = head.to_vec();
        reply.extend_from_slice(bad);
        assert!(Decoder::default().push(&reply).is_err(), "{bad:?}");
    }
    let mut endless = head.to_vec();
    endless.extend(std::iter::repeat_n(b'1', 2048));
    assert!(Decoder::default().push(&endless).is_err());
    let endless_head = vec![b'x'; HEAD_LIMIT + 1];
    assert!(Decoder::default().push(&endless_head).is_err());
}

#[test]
fn the_request_head_carries_the_key_and_the_first_message() {
    let settings = crate::boxd::tests::settings("https://boxd.sh:9443");
    let key = SecretString::from("fixture-key");
    let head = open(&settings, &key, "m-1", "cat");
    let text = String::from_utf8_lossy(&head);
    assert!(text.starts_with("POST /boxd.api.v1.BoxdApi/Exec HTTP/1.1\r\nHost: boxd.sh:9443\r\n"));
    assert!(text.contains("Transfer-Encoding: chunked\r\n"));
    assert!(text.contains("Authorization: Bearer fixture-key\r\n\r\n"));
    let first = frame(&wire::exec_start("m-1", "cat"));
    assert!(
        head.ends_with(&[format!("{:x}\r\n", first.len()).as_bytes(), &first, b"\r\n"].concat())
    );
    let mut chunk = Vec::new();
    stdin(&mut chunk, b"hi");
    assert_eq!(
        chunk,
        [&b"b\r\n"[..], &frame(&wire::exec_stdin(b"hi")), b"\r\n"].concat()
    );
}
