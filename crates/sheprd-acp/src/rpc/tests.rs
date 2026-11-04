#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{io::Cursor, time::Duration};

use crossbeam_channel::bounded;
use serde_json::Value;

use super::*;

#[test]
fn lines_are_classified_by_their_shape() {
    assert!(matches!(
        classify(br#"{"jsonrpc":"2.0","id":3,"method":"session/request_permission","params":{}}"#),
        Some(Inbound::Request { id: wire::RequestId::Number(3), method, .. })
            if method == "session/request_permission"
    ));
    assert!(matches!(
        classify(br#"{"jsonrpc":"2.0","method":"session/update","params":{"a":1}}"#),
        Some(Inbound::Notification { method, params }) if method == "session/update" && params["a"] == 1
    ));
    assert!(matches!(
        classify(br#"{"jsonrpc":"2.0","id":"x","result":{"ok":true}}"#),
        Some(Inbound::Response { id: wire::RequestId::Str(id), result: Ok(_) }) if id == "x"
    ));
    assert!(matches!(
        classify(br#"{"jsonrpc":"2.0","id":4,"error":{"code":-32000,"message":"auth"}}"#),
        Some(Inbound::Response { result: Err(error), .. }) if error.code == wire::Error::auth_required().code
    ));
    assert!(classify(b"npm warn deprecated").is_none());
    assert!(classify(b"[1,2]").is_none());
    assert!(classify(br#"{"result":1}"#).is_none());
}

#[test]
fn the_reader_delivers_messages_then_closes() {
    let input = b"noise\n{\"jsonrpc\":\"2.0\",\"method\":\"m\"}\n{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":null}".to_vec();
    let (sender, receiver) = bounded(8);
    spawn_reader(Cursor::new(input), move |inbound| {
        sender.send(inbound).is_ok()
    })
    .unwrap();
    let received: Vec<_> = receiver.iter().take(3).collect();
    assert!(matches!(received[0], Inbound::Notification { .. }));
    assert!(matches!(received[1], Inbound::Response { .. }));
    assert!(matches!(received[2], Inbound::Closed(None)));
}

#[test]
fn an_overlong_line_closes_with_an_error() {
    let input = vec![b'x'; MAX_LINE + 2];
    let (sender, receiver) = bounded(8);
    spawn_reader(Cursor::new(input), move |inbound| {
        sender.send(inbound).is_ok()
    })
    .unwrap();
    let closed = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(matches!(
        closed,
        Inbound::Closed(Some(Error::LineTooLong { limit: MAX_LINE }))
    ));
}

#[test]
fn messages_are_single_json_rpc_lines() {
    let (sender, receiver) = bounded(8);
    let outbox = Outbox(sender);
    outbox
        .request(7, "session/new", &serde_json::json!({"cwd": "/"}))
        .unwrap();
    outbox
        .notify("session/cancel", &serde_json::json!({}))
        .unwrap();
    outbox
        .respond(
            &wire::RequestId::Number(2),
            Err(wire::Error::method_not_found()),
        )
        .unwrap();
    let lines: Vec<Vec<u8>> = receiver.try_iter().collect();
    for line in &lines {
        assert_eq!(line.last(), Some(&b'\n'));
        assert_eq!(line.iter().filter(|byte| **byte == b'\n').count(), 1);
    }
    let request = serde_json::from_slice::<Value>(&lines[0]).unwrap();
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["id"], 7);
    let error = serde_json::from_slice::<Value>(&lines[2]).unwrap();
    assert_eq!(error["error"]["code"], -32601);
}

#[test]
fn a_full_outbox_reports_a_stalled_agent() {
    let (sender, _receiver) = bounded(1);
    let outbox = Outbox(sender);
    outbox.notify("a", &()).unwrap();
    assert!(matches!(outbox.notify("b", &()), Err(Error::AgentStalled)));
}

#[cfg(unix)]
#[test]
fn a_real_child_round_trips_through_reader_and_writer() {
    let mut child = sheprd_client::new_command("cat")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let (outbox, _writer) = spawn_writer(child.stdin.take().unwrap()).unwrap();
    let (sender, receiver) = bounded(8);
    spawn_reader(child.stdout.take().unwrap(), move |inbound| {
        sender.send(inbound).is_ok()
    })
    .unwrap();
    outbox
        .request(1, "echo", &serde_json::json!({"x": 1}))
        .unwrap();
    let echoed = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(matches!(echoed, Inbound::Request { method, .. } if method == "echo"));
    drop(outbox);
    let closed = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(matches!(closed, Inbound::Closed(None)));
    child.wait().unwrap();
}
