#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{
    io::{BufRead, BufReader, PipeReader, PipeWriter},
    time::Duration,
};

use acp::v1 as wire;
use crossbeam_channel::unbounded;
use serde_json::json;

use super::*;

const WAIT: Duration = Duration::from_secs(10);

/// A file link under the test checkout, absolute on this platform: a
/// Windows file URI needs a drive to name a path.
pub(crate) fn file_uri(name: &str) -> String {
    if cfg!(windows) {
        format!("file:///C:/work/{name}")
    } else {
        format!("file:///work/{name}")
    }
}

/// The path [`file_uri`] names, as the platform writes it.
pub(crate) fn file_name(name: &str) -> String {
    if cfg!(windows) {
        format!("C:\\work\\{}", name.replace('/', "\\"))
    } else {
        format!("/work/{name}")
    }
}

/// The far end of a harness's pipes, and the session's side of the reach.
struct Wired {
    delivered: Receiver<Inbound>,
    harness_input: BufReader<PipeReader>,
    harness_output: PipeWriter,
    outbox: rpc::Outbox,
}

fn wire(reach: Box<dyn Reach>) -> Wired {
    let (harness_input, input) = std::io::pipe().unwrap();
    let (output, harness_output) = std::io::pipe().unwrap();
    let (deliver, delivered) = unbounded();
    let outbox = reach
        .connect(
            Pipes {
                input: Box::new(input),
                output: Box::new(output),
            },
            Box::new(move |inbound| deliver.send(inbound).is_ok()),
        )
        .unwrap();
    Wired {
        delivered,
        harness_input: BufReader::new(harness_input),
        harness_output,
        outbox,
    }
}

impl Wired {
    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.harness_input.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn write(&mut self, line: &str) {
        self.harness_output.write_all(line.as_bytes()).unwrap();
        self.harness_output.write_all(b"\n").unwrap();
    }

    fn next(&self) -> Inbound {
        self.delivered.recv_timeout(WAIT).unwrap()
    }
}

#[test]
fn an_acp_harness_is_wired_straight_to_the_session() {
    let mut wired = wire(Box::new(Acp));
    wired
        .outbox
        .notify("session/cancel", &json!({"sessionId": "s"}))
        .unwrap();
    assert_eq!(wired.read()["method"], "session/cancel");
    wired.write(r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#);
    assert!(matches!(
        wired.next(),
        Inbound::Response {
            id: wire::RequestId::Number(1),
            result: Ok(_)
        }
    ));
    drop(wired.harness_output);
    assert!(matches!(
        wired.delivered.recv_timeout(WAIT).unwrap(),
        Inbound::Closed(None)
    ));
}

/// Answers every session request with what the harness says next, and says
/// hello first.
#[derive(Default)]
struct Relay {
    waiting: Option<wire::RequestId>,
}

impl Translator for Relay {
    fn start(&mut self) -> Vec<Out> {
        vec![Out::Harness(json!({"hello": true}))]
    }

    fn on_session(&mut self, inbound: Inbound) -> Vec<Out> {
        let Inbound::Request { id, method, .. } = inbound else {
            return Vec::new();
        };
        self.waiting = Some(id);
        vec![Out::Harness(json!({"asked": method}))]
    }

    fn on_harness(&mut self, message: &Value) -> Vec<Out> {
        self.waiting
            .take()
            .map(|id| {
                Out::Session(Inbound::Response {
                    id,
                    result: Ok(message.clone()),
                })
            })
            .into_iter()
            .collect()
    }
}

#[test]
fn a_translator_sits_between_the_session_and_the_harness() {
    let mut wired = wire(Box::new(Translated(Relay::default())));
    assert_eq!(wired.read(), json!({"hello": true}), "its start goes first");
    wired
        .outbox
        .request(7, "session/prompt", &json!({}))
        .unwrap();
    assert_eq!(wired.read(), json!({"asked": "session/prompt"}));
    wired.write("not json, skipped");
    wired.write(r#"{"answer": 42}"#);
    match wired.next() {
        Inbound::Response {
            id: wire::RequestId::Number(7),
            result: Ok(answer),
        } => {
            assert_eq!(answer, json!({"answer": 42}));
        }
        other => panic!("{other:?}"),
    }
    // The harness going away ends the translator and tells the session.
    drop(wired.harness_output);
    assert!(matches!(
        wired.delivered.recv_timeout(WAIT).unwrap(),
        Inbound::Closed(None)
    ));
}

#[test]
fn file_links_name_their_path_and_long_commands_say_so() {
    assert_eq!(
        out::file_path(&file_uri("My%20File.md")),
        file_name("My File.md")
    );
    assert_eq!(
        out::file_path("https://example.com/a"),
        "https://example.com/a"
    );
    assert_eq!(out::command_title("ls"), "`ls`");
    assert_eq!(out::command_title("a\nb\nc"), "`a` (+2 more lines)");
}
