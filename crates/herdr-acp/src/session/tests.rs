#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{cell::RefCell, rc::Rc};

use crossbeam_channel::{Receiver, bounded};
use herdr_chat_protocol::{EntryKind, NoticeLevel, PermissionId};
use serde_json::{Value, json};

use super::*;
use crate::rpc::{Inbound, Outbox};

mod attach;
mod attachments;
mod auto_accept;
mod bounds;
mod commands;
mod flow;
mod linked_pictures;
mod media;
mod permissions;
mod prompt_titles;
mod settings;

type Reports = Rc<RefCell<Vec<Activity>>>;
type Prompts = Rc<RefCell<Vec<String>>>;
type TestSession = Session<Box<dyn FnMut(Update)>>;

struct Harness {
    agent: Receiver<Vec<u8>>,
    /// The prompts passed on for Herdr.
    prompts: Prompts,
    reports: Reports,
    session: TestSession,
}

fn config() -> Config {
    Config {
        agent: "Claude".to_owned(),
        cwd: PathBuf::from("/work"),
        exit_with_agent: false,
        load: None,
        login_hint: Some("run `claude` and use /login".to_owned()),
        pane: Some("p_1".to_owned()),
    }
}

fn harness_with(config: Config) -> Harness {
    let (sender, agent) = bounded(64);
    let reports: Reports = Rc::default();
    let prompts: Prompts = Rc::default();
    let (sink, prompt_sink) = (Rc::clone(&reports), Rc::clone(&prompts));
    let session = Session::new(
        config,
        Outbox(sender),
        Box::new(move |update| match update {
            Update::Activity(activity) => sink.borrow_mut().push(activity),
            Update::Prompt(prompt) => prompt_sink.borrow_mut().push(prompt),
        }) as Box<dyn FnMut(Update)>,
    );
    Harness {
        agent,
        prompts,
        reports,
        session,
    }
}

impl Harness {
    /// Every message the session wrote to the agent since the last call.
    fn sent(&self) -> Vec<Value> {
        self.agent
            .try_iter()
            .map(|line| serde_json::from_slice(&line).unwrap())
            .collect()
    }

    fn last_request(&self, method: &str) -> Value {
        self.sent()
            .into_iter()
            .rev()
            .find(|message| message["method"] == method)
            .unwrap_or_else(|| panic!("no {method} request"))
    }

    fn input(&mut self, input: Input) -> Flow {
        self.session.handle(input)
    }

    fn respond(&mut self, id: i64, result: Value) {
        self.input(Input::Agent(Inbound::Response {
            id: wire::RequestId::Number(id),
            result: Ok(result),
        }));
    }

    fn fail(&mut self, id: i64, error: wire::Error) {
        self.input(Input::Agent(Inbound::Response {
            id: wire::RequestId::Number(id),
            result: Err(error),
        }));
    }

    fn update(&mut self, update: Value) {
        self.input(Input::Agent(Inbound::Notification {
            method: "session/update".to_owned(),
            params: json!({"sessionId": "s1", "update": update}),
        }));
    }

    fn agent_says(&mut self, text: &str) {
        self.update(json!({"sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": text}}));
    }

    fn subscribe(
        &mut self,
        conn: u64,
        local: bool,
        since: Option<Cursor>,
    ) -> Receiver<ServerMessage> {
        let (outbox, receiver) = bounded(64);
        self.input(Input::Attach(Attach {
            conn: ConnId(conn),
            local,
            outbox,
            since,
        }));
        receiver
    }

    fn command(&mut self, conn: u64, command: Command) -> CommandId {
        let id = CommandId::random();
        self.input(Input::Command {
            command,
            conn: ConnId(conn),
            id,
        });
        id
    }

    fn transcript(&self) -> &Transcript {
        self.session.transcript()
    }

    fn notices(&self) -> Vec<(NoticeLevel, String)> {
        self.transcript()
            .entries
            .iter()
            .filter_map(|entry| match &entry.kind {
                EntryKind::Notice { level, text, .. } => Some((*level, text.clone())),
                _ => None,
            })
            .collect()
    }
}

/// A session whose agent answered the handshake and opened session `s1`
/// with two modes.
fn ready() -> Harness {
    let mut harness = harness_with(config());
    harness.session.start();
    let initialize = harness.last_request("initialize");
    assert_eq!(initialize["params"]["protocolVersion"], 1);
    harness.respond(
        initialize["id"].as_i64().unwrap(),
        json!({"protocolVersion": 1, "agentCapabilities": {}}),
    );
    let new = harness.last_request("session/new");
    assert_eq!(new["params"]["cwd"], "/work");
    harness.respond(
        new["id"].as_i64().unwrap(),
        json!({"sessionId": "s1", "modes": {"currentModeId": "default", "availableModes": [
            {"id": "default", "name": "Default"}, {"id": "plan", "name": "Plan"}]}}),
    );
    harness
}

/// Sends a prompt from `conn` and returns the `session/prompt` request id.
fn prompt(harness: &mut Harness, conn: u64, text: &str) -> i64 {
    harness.command(
        conn,
        Command::Prompt {
            text: text.to_owned(),
            attachments: Vec::new(),
        },
    );
    harness.last_request("session/prompt")["id"]
        .as_i64()
        .unwrap()
}

fn ask_permission(harness: &mut Harness, request: i64) -> PermissionId {
    harness.input(Input::Agent(Inbound::Request {
        id: wire::RequestId::Number(request),
        method: "session/request_permission".to_owned(),
        params: json!({"sessionId": "s1",
            "toolCall": {"toolCallId": "call-1", "title": "Run cargo test", "kind": "execute"},
            "options": [
                {"optionId": "allow", "name": "Allow", "kind": "allow_once"},
                {"optionId": "deny", "name": "Deny", "kind": "reject_once"}]}),
    }));
    harness.transcript().permissions.last().unwrap().id
}

fn drain(receiver: &Receiver<ServerMessage>) -> Vec<ServerMessage> {
    receiver.try_iter().collect()
}
