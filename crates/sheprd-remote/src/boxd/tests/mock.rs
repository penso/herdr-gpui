//! A scripted grpc-web API on loopback, for exercising the real HTTP path:
//! each call is recorded with its method and decoded request, and answered
//! by the test's handler.
#![allow(clippy::unwrap_used)]

use super::super::{
    Error, Settings, Status,
    error::Code,
    http::Request,
    wire::{self, Field, Message, fields},
};
use secrecy::SecretString;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};

#[derive(Clone, Debug)]
pub struct Call {
    pub method: String,
    pub head: String,
    /// The request message, out of its frame.
    pub message: Vec<u8>,
}

/// A reply: a message with status 0, a status alone, or raw HTTP.
pub enum Reply {
    Message(Vec<u8>),
    Refused(u32, &'static str),
    Http(u16),
}

fn response(reply: Reply) -> Vec<u8> {
    let (head, body) = match reply {
        Reply::Message(message) => {
            let mut body = wire::frame(&message);
            let trailer = b"grpc-status:0\r\n";
            body.push(wire::TRAILER);
            body.extend_from_slice(&(trailer.len() as u32).to_be_bytes());
            body.extend_from_slice(trailer);
            (
                "HTTP/1.1 200 OK\r\ncontent-type: application/grpc-web+proto\r\n".to_owned(),
                body,
            )
        }
        Reply::Refused(code, message) => (
            format!("HTTP/1.1 200 OK\r\ngrpc-status: {code}\r\ngrpc-message: {message}\r\n"),
            Vec::new(),
        ),
        Reply::Http(code) => (format!("HTTP/1.1 {code} X\r\n"), Vec::new()),
    };
    let mut bytes = format!(
        "{head}content-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend(body);
    bytes
}

fn serve(handler: impl Fn(&Call) -> Reply + Send + 'static) -> (String, Arc<Mutex<Vec<Call>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut bytes = Vec::new();
            let mut chunk = [0; 4096];
            let end = loop {
                let read = stream.read(&mut chunk).unwrap();
                if read == 0 {
                    break None;
                }
                bytes.extend_from_slice(&chunk[..read]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break Some(end + 4);
                }
            };
            let Some(end) = end else { continue };
            let head = String::from_utf8_lossy(&bytes[..end]).to_string();
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            while bytes.len() < end + length {
                let read = stream.read(&mut chunk).unwrap();
                bytes.extend_from_slice(&chunk[..read]);
            }
            let path = head.split(' ').nth(1).unwrap_or_default();
            let (_, payload, _) = wire::take_frame(&bytes[end..]).unwrap().unwrap();
            let call = Call {
                method: path.rsplit('/').next().unwrap().to_owned(),
                head: head.clone(),
                message: payload.to_vec(),
            };
            let reply = handler(&call);
            seen.lock().unwrap().push(call);
            stream.write_all(&response(reply)).unwrap();
        }
    });
    (base, log)
}

fn machine(id: &str, status: &str) -> Vec<u8> {
    Message::default()
        .string(1, id)
        .string(2, "sheprd-calm-otter")
        .string(5, status)
        .into_bytes()
}

fn string_field(message: &[u8], number: u32) -> String {
    fields(message)
        .unwrap()
        .into_iter()
        .find_map(|(n, field)| match field {
            Field::Bytes(value) if n == number => Some(String::from_utf8(value.to_vec()).unwrap()),
            _ => None,
        })
        .unwrap_or_default()
}

fn api(handler: impl Fn(&Call) -> Reply + Send + 'static) -> (Settings, Arc<Mutex<Vec<Call>>>) {
    let (base, log) = serve(handler);
    let mut settings = super::settings(&base);
    settings.labels = [("team".to_owned(), "web".to_owned())].into();
    (settings, log)
}

#[test]
fn creating_sends_the_key_name_and_labels_as_grpc_web() {
    let (settings, log) = api(|call| match call.method.as_str() {
        "CreateVm" => Reply::Message(
            Message::default()
                .string(1, "id-1")
                .string(2, "sheprd-calm-otter")
                .string(6, "running")
                .into_bytes(),
        ),
        other => panic!("unexpected {other}"),
    });
    let key = SecretString::from("fixture-key");
    let created = Request {
        settings: &settings,
        key: &key,
    }
    .create("sheprd-calm-otter")
    .unwrap();
    assert_eq!(
        (created.id.as_str(), created.status.as_str()),
        ("id-1", "running")
    );
    let log = log.lock().unwrap();
    let head = log[0].head.to_ascii_lowercase();
    assert!(head.starts_with("post /boxd.api.v1.boxdapi/createvm "));
    assert!(head.contains("authorization: bearer fixture-key"));
    assert!(head.contains("content-type: application/grpc-web+proto"));
    assert_eq!(string_field(&log[0].message, 1), "sheprd-calm-otter");
    let labels: Vec<(String, String)> = fields(&log[0].message)
        .unwrap()
        .into_iter()
        .filter_map(|(number, field)| match field {
            Field::Bytes(entry) if number == 14 => {
                Some((string_field(entry, 1), string_field(entry, 2)))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        labels,
        [
            ("team".to_owned(), "web".to_owned()),
            ("sheprd".to_owned(), "device".to_owned())
        ]
    );
}

#[test]
fn listing_selects_the_device_label() {
    let (settings, log) = api(|_| {
        Reply::Message(
            Message::default()
                .bytes(1, &machine("a", "running"))
                .bytes(1, &machine("b", "hibernated"))
                .into_bytes(),
        )
    });
    let key = SecretString::from("k");
    let machines = Request {
        settings: &settings,
        key: &key,
    }
    .machines()
    .unwrap();
    assert_eq!(machines.len(), 2);
    let log = log.lock().unwrap();
    assert_eq!(log[0].method, "ListVms");
    assert_eq!(string_field(&log[0].message, 3), "sheprd=device");
}

#[test]
fn waiting_wakes_a_hibernated_machine_once() {
    let polls = Arc::new(Mutex::new(0));
    let counter = polls.clone();
    let (settings, log) = api(move |call| match call.method.as_str() {
        "GetVm" => {
            let mut polls = counter.lock().unwrap();
            *polls += 1;
            Reply::Message(machine(
                "id-1",
                match *polls {
                    1 => "hibernated",
                    2 => "starting",
                    _ => "running",
                },
            ))
        }
        "WakeVm" => Reply::Message(Vec::new()),
        other => panic!("unexpected {other}"),
    });
    let key = SecretString::from("k");
    let seen = Mutex::new(Vec::new());
    let ready = Request {
        settings: &settings,
        key: &key,
    }
    .wait_ready(
        "id-1",
        || false,
        |readiness| seen.lock().unwrap().push(readiness),
    )
    .unwrap();
    assert_eq!(ready.status, "running");
    let methods: Vec<String> = log
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.method.clone())
        .collect();
    assert_eq!(methods, ["GetVm", "WakeVm", "GetVm", "GetVm"]);
    use super::super::api::Readiness;
    assert_eq!(
        *seen.lock().unwrap(),
        [Readiness::Hibernated, Readiness::Busy]
    );
}

#[test]
fn a_stopped_machine_is_started_and_a_cancelled_wait_stops() {
    let (settings, log) = api(|call| match call.method.as_str() {
        "GetVm" => Reply::Message(machine("id-1", "stopped")),
        "StartVm" => Reply::Message(Vec::new()),
        other => panic!("unexpected {other}"),
    });
    let key = SecretString::from("k");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let calls = std::cell::Cell::new(0);
    let cancelled = || {
        calls.set(calls.get() + 1);
        calls.get() > 1
    };
    assert!(matches!(
        request.wait_ready("id-1", cancelled, |_| {}),
        Err(Error::Cancelled)
    ));
    let methods: Vec<String> = log
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.method.clone())
        .collect();
    assert_eq!(methods, ["GetVm", "StartVm"]);
}

#[test]
fn refusals_become_typed_errors() {
    let (settings, _) = api(|call| match call.method.as_str() {
        "GetVm" => Reply::Refused(5, "machine%20not%20found"),
        "DestroyVm" => Reply::Refused(5, "gone"),
        "ListVms" => Reply::Refused(16, "API%20key%20not%20recognized"),
        "StartVm" => Reply::Http(502),
        other => panic!("unexpected {other}"),
    });
    let key = SecretString::from("k");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    assert!(matches!(request.machine("id-1"), Err(Error::Gone(id)) if id == "id-1"));
    request.delete("id-1").unwrap();
    assert!(matches!(request.machines(), Err(Error::Authentication)));
    assert!(matches!(
        request.call("test", "StartVm", &[]),
        Err(Error::Status(Status {
            code: Code::Http(502),
            ..
        }))
    ));
    assert!(matches!(request.machine("../x"), Err(Error::Field(_))));
    assert!(matches!(request.create("Bad Name"), Err(Error::Field(_))));
}
