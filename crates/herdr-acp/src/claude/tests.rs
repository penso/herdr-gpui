#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    time::Duration,
};

use crossbeam_channel::{Receiver, unbounded};
use herdr_chat_protocol::{Command, CommandId, EntryKind, Status};
use serde_json::{Value, json};

use super::*;
use crate::session::{Config, ConnId, Input, Session};
use crate::{harness::Pipes, rpc::Inbound};

const WAIT: Duration = Duration::from_secs(10);

#[test]
fn claude_runs_in_its_streaming_mode_for_one_session() {
    let new: Vec<String> = args("s-1", false)
        .into_iter()
        .map(|arg| arg.into_string().unwrap())
        .collect();
    for flag in [
        "-p",
        "--verbose",
        "--include-partial-messages",
        "--allow-dangerously-skip-permissions",
    ] {
        assert!(new.iter().any(|arg| arg == flag), "{flag}");
    }
    let pair = |args: &[String], flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .map(|at| args[at + 1].clone())
    };
    assert_eq!(pair(&new, "--input-format").as_deref(), Some("stream-json"));
    assert_eq!(
        pair(&new, "--output-format").as_deref(),
        Some("stream-json")
    );
    assert_eq!(
        pair(&new, "--permission-prompt-tool").as_deref(),
        Some("stdio")
    );
    assert_eq!(pair(&new, "--session-id").as_deref(), Some("s-1"));
    let resumed: Vec<String> = args("s-1", true)
        .into_iter()
        .map(|arg| arg.into_string().unwrap())
        .collect();
    assert_eq!(pair(&resumed, "--resume").as_deref(), Some("s-1"));
    assert!(pair(&resumed, "--session-id").is_none());
}

/// Claude's side of the pipes.
struct FakeClaude {
    input: BufReader<std::io::PipeReader>,
    output: std::io::PipeWriter,
}

impl FakeClaude {
    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.input.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn write(&mut self, message: &Value) {
        let mut line = serde_json::to_vec(message).unwrap();
        line.push(b'\n');
        self.output.write_all(&line).unwrap();
    }
}

/// Feeds what the bridge delivers to the session until `done` holds.
fn pump<R: FnMut(crate::session::Update)>(
    session: &mut Session<R>,
    delivered: &Receiver<Inbound>,
    done: impl Fn(&Session<R>) -> bool,
) {
    while !done(session) {
        let inbound = delivered.recv_timeout(WAIT).expect("the bridge delivers");
        session.handle(Input::Agent(inbound));
    }
}

#[test]
fn a_whole_turn_reaches_the_transcript_through_the_bridge() {
    let (stdin_reader, stdin_writer) = std::io::pipe().unwrap();
    let (stdout_reader, stdout_writer) = std::io::pipe().unwrap();
    let (deliver, delivered) = unbounded();
    let outbox = reach("s-1".to_owned())
        .connect(
            Pipes {
                input: Box::new(stdin_writer),
                output: Box::new(stdout_reader),
            },
            Box::new(move |inbound| deliver.send(inbound).is_ok()),
        )
        .unwrap();
    let mut claude = FakeClaude {
        input: BufReader::new(stdin_reader),
        output: stdout_writer,
    };
    let config = Config {
        agent: "Claude".to_owned(),
        cwd: PathBuf::from("/work"),
        exit_with_agent: false,
        load: None,
        login_hint: None,
        pane: None,
    };
    let mut session = Session::new(config, outbox, |_| {});
    session.start();

    let initialize = claude.read();
    assert_eq!(initialize["request"]["subtype"], "initialize");
    claude.write(&json!({
        "type": "control_response",
        "response": {
            "subtype": "success",
            "request_id": initialize["request_id"],
            "response": {
                "commands": [{"name": "review", "description": "Review", "argumentHint": ""}],
                "models": [{"value": "default", "displayName": "Default", "resolvedModel": "claude-opus-5-5"}],
                "current_permission_mode": "default",
            },
        },
    }));
    pump(&mut session, &delivered, |session| {
        session.transcript().status == Status::Idle
    });
    let transcript = session.transcript();
    assert_eq!(transcript.session.as_ref().unwrap().id, "s-1");
    assert_eq!(transcript.modes.as_ref().unwrap().current, "default");
    assert_eq!(transcript.options.len(), 1);

    session.handle(Input::Command {
        command: Command::Prompt {
            text: "Say hello".to_owned(),
            attachments: Vec::new(),
        },
        conn: ConnId(1),
        id: CommandId(uuid::Uuid::new_v4()),
    });
    let prompt = claude.read();
    assert_eq!(prompt["message"]["content"][0]["text"], "Say hello");
    for message in [
        json!({"type": "system", "subtype": "init", "model": "claude-opus-5-5", "permissionMode": "default"}),
        json!({"type": "stream_event", "event": {"type": "message_start", "message": {"id": "m1"}}}),
        json!({"type": "stream_event", "event": {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "Hel"}}}),
        json!({"type": "stream_event", "event": {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "lo"}}}),
        json!({"type": "assistant", "message": {"id": "m1", "content": [{"type": "text", "text": "Hello"}]}}),
        json!({"type": "result", "subtype": "success", "is_error": false, "stop_reason": "end_turn",
               "usage": {"input_tokens": 3, "output_tokens": 2}}),
    ] {
        claude.write(&message);
    }
    pump(&mut session, &delivered, |session| {
        session.transcript().status == Status::Idle && session.transcript().turn.is_none()
    });
    let kinds: Vec<&EntryKind> = session
        .transcript()
        .entries
        .iter()
        .map(|entry| &entry.kind)
        .collect();
    assert!(matches!(kinds[0], EntryKind::User { text, .. } if text == "Say hello"));
    assert!(matches!(kinds[1], EntryKind::Agent { text } if text == "Hello"));
    assert!(
        matches!(kinds[2], EntryKind::Turn(summary) if summary.tokens.is_some_and(|tokens| tokens.output == 2))
    );

    // Claude going away ends the bridge and tells the session.
    drop(claude);
    let closed = delivered.recv_timeout(WAIT).unwrap();
    assert!(matches!(closed, Inbound::Closed(_)));
}
