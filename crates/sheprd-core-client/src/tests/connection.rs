use super::{MockCore, connected, hello_reply, pair, terminal};
use crate::{Core, End, Error, Method, frame::Kind, hello::Mismatch, wire::Attach};
use serde_json::{Value, json};
use std::{thread, time::Duration};

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn the_handshake_asks_for_framed_mode_and_checks_the_core() {
    let (client, server) = pair();
    let core = thread::spawn(move || {
        MockCore::accept(server, &hello_reply("0.1.13", &super::every_method()))
    });
    let connected = Core::over(client).unwrap();
    let (_, hello) = core.join().unwrap();
    assert_eq!(hello["method"], "hello");
    assert_eq!(hello["params"], json!({"framed": true, "generation": 1}));
    assert_eq!(connected.hello().core.name, "box");

    let (client, server) = pair();
    let core = thread::spawn(move || {
        MockCore::accept(server, &hello_reply("0.1.0", &super::every_method()))
    });
    assert!(matches!(
        Core::over(client),
        Err(Error::Compat(Mismatch::TooOld { .. }))
    ));
    drop(core.join().unwrap());
}

#[test]
fn calls_are_answered_by_id_even_out_of_order() {
    let (client, mut core) = connected();
    let client = std::sync::Arc::new(client);
    let first = thread::spawn({
        let client = client.clone();
        move || client.doc_get("one").unwrap()
    });
    let one = core.request();
    let second = thread::spawn({
        let client = client.clone();
        move || client.doc_get("two").unwrap()
    });
    let two = core.request();
    assert_eq!(
        (
            one["params"]["name"].as_str(),
            two["params"]["name"].as_str()
        ),
        (Some("one"), Some("two"))
    );
    core.answer(&two, json!({"value": 2, "version": 2}));
    core.answer(&one, json!({"value": 1, "version": 1}));
    assert_eq!(first.join().unwrap().value, json!(1));
    assert_eq!(second.join().unwrap().value, json!(2));
}

#[test]
fn a_refusal_keeps_the_cores_kind() {
    let (client, mut core) = connected();
    let call = thread::spawn(move || client.kill("t9"));
    let request = core.request();
    let message = json!({"jsonrpc": "2.0", "id": request["id"],
        "error": {"code": -32000, "message": "no terminal t9", "data": {"kind": "not_found", "hint": "hc ls"}}});
    core.send(0, Kind::Data, &serde_json::to_vec(&message).unwrap());
    match call.join().unwrap() {
        Err(Error::Refused {
            method: Method::TerminalKill,
            kind,
            message,
        }) => {
            assert_eq!(
                (kind.as_deref(), message.as_str()),
                (Some("not_found"), "no terminal t9")
            );
        }
        other => panic!("{other:?}"),
    }
}

/// Attaches `t7` on channel 5; `then` runs right after the core answers.
fn attached(
    client: Core,
    core: &mut MockCore,
    then: impl FnOnce(&mut MockCore),
) -> (Core, crate::Attachment) {
    let attach = thread::spawn(move || {
        let attachment = client.attach(&Attach {
            cols: 80,
            rows: 24,
            terminal: "t7",
            ..Attach::default()
        });
        (client, attachment)
    });
    let request = core.request();
    assert_eq!(
        request["params"],
        json!({"cols": 80, "rows": 24, "terminal": "t7"})
    );
    core.answer(&request, json!({"channel": 5, "terminal": terminal("t7")}));
    then(core);
    let (client, attachment) = attach.join().unwrap();
    (client, attachment.unwrap())
}

#[test]
fn stream_bytes_that_follow_the_answer_are_kept_and_credited() {
    let (client, mut core) = connected();
    let (_client, attachment) = attached(client, &mut core, |core| {
        core.send(5, Kind::Data, b"\x1b[2Jhello")
    });
    let chunk = attachment.read(WAIT).unwrap();
    assert_eq!(
        (chunk.bytes.as_slice(), chunk.end),
        (&b"\x1b[2Jhello"[..], None)
    );
    let credit = core.frame();
    assert_eq!(
        (credit.channel, credit.kind, credit.payload),
        (5, Kind::Credit, 9u32.to_le_bytes().to_vec())
    );
}

#[test]
fn viewer_frames_carry_keys_size_and_focus() {
    let (client, mut core) = connected();
    let (_client, attachment) = attached(client, &mut core, |_| {});
    attachment.write(b"ls\r").unwrap();
    attachment.resize(100, 30).unwrap();
    attachment.focus(true).unwrap();
    let frames: Vec<_> = (0..3)
        .map(|_| core.frame())
        .map(|frame| (frame.channel, frame.kind, frame.payload))
        .collect();
    let mut size = 100u16.to_le_bytes().to_vec();
    size.extend_from_slice(&30u16.to_le_bytes());
    assert_eq!(
        frames,
        [
            (5, Kind::Data, b"ls\r".to_vec()),
            (5, Kind::Resize, size),
            (5, Kind::Focus, vec![1])
        ]
    );
    drop(attachment);
    let close = core.frame();
    assert_eq!((close.channel, close.kind), (5, Kind::Close));
}

#[test]
fn a_closed_stream_reports_why_once() {
    let (client, mut core) = connected();
    let (_client, attachment) = attached(client, &mut core, |core| {
        core.send(
            5,
            Kind::Close,
            br#"{"reason":"exited","exit_code":3,"signal":null}"#,
        )
    });
    let chunk = attachment.read(WAIT).unwrap();
    assert!(matches!(chunk.end, Some(End::Closed(closed)) if closed.exit_code == Some(3)));
    assert_eq!(
        attachment.read(Duration::from_millis(20)).unwrap().end,
        None
    );
}

#[test]
fn losing_the_core_fails_waiting_calls_and_ends_streams() {
    let (client, mut core) = connected();
    let client = std::sync::Arc::new(client);
    let call = thread::spawn({
        let client = client.clone();
        move || client.list(false)
    });
    let _ = core.request();
    drop(core);
    assert!(matches!(call.join().unwrap(), Err(Error::Lost)));
    assert!(client.is_lost());
    assert!(matches!(client.doc_get("x"), Err(Error::Lost)));
}

#[test]
fn a_subscription_turns_lines_into_notices() {
    let (client, mut core) = connected();
    let subscribe = thread::spawn(move || {
        let subscription = client.subscribe();
        (client, subscription)
    });
    let request = core.request();
    core.answer(
        &request,
        json!({"channel": 3, "seq": 10, "terminals": [terminal("t1")]}),
    );
    let (_client, subscription) = subscribe.join().unwrap();
    let mut subscription = subscription.unwrap();
    assert_eq!((subscription.seq, subscription.terminals.len()), (10, 1));
    let line = |value: Value| [serde_json::to_vec(&value).unwrap(), b"\n".to_vec()].concat();
    let bytes = [line(json!({"method": "heartbeat", "params": {"seq": 11}})),
        line(json!({"method": "event", "params": {"seq": 12, "type": "title", "terminal": "t1", "data": {"title": "x"}}}))].concat();
    let (head, tail) = bytes.split_at(7);
    core.send(3, Kind::Data, head);
    core.send(3, Kind::Data, tail);
    let mut notices = Vec::new();
    while notices.len() < 2 {
        notices.extend(subscription.next(WAIT).unwrap());
    }
    assert_eq!(notices[0], crate::event::Notice::Heartbeat { seq: 11 });
    core.send(3, Kind::Close, b"");
    let ended = (0..100).find_map(|_| subscription.next(Duration::from_millis(50)).err());
    assert!(matches!(ended, Some(Error::Lost)));
}
