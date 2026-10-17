//! Routing and the accept loop. One thread per connection, bounded by
//! `max_connections`: a hook connection deliberately stays open while it waits
//! for the phone, so the cap is what keeps that waiting bounded.

use crate::{
    Error, Result,
    broker::{Broker, Decision, EventKind, Limits, Origin},
    herdr_api::{Agent, Herdr, StartAgent},
    hook::{self, Action, Hook},
    http::{self, Method, Request, Response},
    notify::{Notifier, notice_for},
    transcript::{self, tool_summary},
    web,
};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    io::BufReader,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

const IO_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_EVENT_WAIT: Duration = Duration::from_secs(30);

pub struct Config {
    /// Every API route, hooks included, requires `Authorization: Bearer <token>`.
    /// Only the web app's static files are public.
    pub token: SecretString,
    /// How long a hook waits for the phone before handing the prompt back to
    /// the terminal. Keep it below the hook's own `timeout`.
    pub decision_timeout: Duration,
    /// Herdr's JSON API socket; `None` disables the routes that need Herdr.
    pub herdr_socket: Option<PathBuf>,
    pub notifier: Option<Notifier>,
    pub limits: Limits,
    pub max_connections: usize,
}

pub struct Companion {
    config: Config,
    broker: Broker,
}

#[derive(Deserialize)]
struct PromptBody {
    text: String,
}

#[derive(Deserialize)]
struct KeysBody {
    keys: Vec<String>,
}

/// An agent as the phone lists it: Herdr's view plus what the hooks know.
#[derive(Serialize)]
struct AgentView {
    #[serde(flatten)]
    agent: Agent,
    workspace_label: Option<String>,
    /// Prompts from this pane waiting for a decision.
    pending: usize,
    has_transcript: bool,
}

impl Companion {
    pub fn new(config: Config) -> Self {
        Self {
            broker: Broker::new(config.limits),
            config,
        }
    }

    pub fn broker(&self) -> &Broker {
        &self.broker
    }

    fn authorized(&self, request: &Request) -> bool {
        let Some(presented) = request
            .header("authorization")
            .and_then(|value| value.strip_prefix("Bearer "))
        else {
            return false;
        };
        constant_time_eq(
            presented.as_bytes(),
            self.config.token.expose_secret().as_bytes(),
        )
    }

    fn herdr(&self) -> Result<Herdr<'_>> {
        self.config
            .herdr_socket
            .as_deref()
            .map(Herdr::new)
            .ok_or(Error::NoHerdrSocket)
    }

    pub(crate) fn handle(&self, request: &Request) -> Response {
        if request.method == Method::Get
            && let Some((content_type, body)) = web::asset(&request.path)
        {
            return Response::asset(content_type, body);
        }
        if !self.authorized(request) {
            return Response::json(401, &json!({ "error": "missing or wrong bearer token" }));
        }
        let segments: Vec<&str> = request.path.trim_matches('/').split('/').collect();
        let ok = || Ok(Response::json(200, &json!({ "ok": true })));
        let result = match (request.method, segments.as_slice()) {
            (Method::Post, ["hooks", "claude"]) => self.claude_hook(request),
            (Method::Get, ["v1", "requests"]) => Ok(Response::json(
                200,
                &json!({ "requests": self.broker.pending() }),
            )),
            (Method::Post, ["v1", "requests", id]) => self.decide(id, request),
            (Method::Get, ["v1", "events"]) => Ok(self.events(request)),
            (Method::Get, ["v1", "sessions"]) => Ok(Response::json(
                200,
                &json!({ "sessions": self.broker.sessions() }),
            )),
            (Method::Get, ["v1", "sessions", id, "transcript"]) => self.transcript(id, request),
            (Method::Get, ["v1", "agents"]) => self.agents(),
            (Method::Post, ["v1", "agents"]) => self.start(request),
            (Method::Get, ["v1", "workspaces"]) => self
                .herdr()
                .and_then(|herdr| herdr.workspaces())
                .map(|workspaces| Response::json(200, &json!({ "workspaces": workspaces }))),
            (Method::Post, ["v1", "panes", pane, "prompt"]) => {
                serde_json::from_slice(&request.body)
                    .map_err(Error::from)
                    .and_then(|body: PromptBody| self.herdr()?.prompt(pane, &body.text))
                    .and_then(|()| ok())
            }
            (Method::Post, ["v1", "panes", pane, "keys"]) => serde_json::from_slice(&request.body)
                .map_err(Error::from)
                .and_then(|body: KeysBody| self.herdr()?.send_keys(pane, &body.keys))
                .and_then(|()| ok()),
            // Esc is how Claude Code and most agents stop a turn; no API
            // method interrupts an agent.
            (Method::Post, ["v1", "panes", pane, "interrupt"]) => self
                .herdr()
                .and_then(|herdr| herdr.send_keys(pane, &["esc".to_owned()]))
                .and_then(|()| ok()),
            (Method::Get, ["v1", "panes", pane, "screen"]) => self
                .herdr()
                .and_then(|herdr| herdr.screen(pane))
                .map(|text| Response::json(200, &json!({ "text": text }))),
            (
                _,
                ["hooks", "claude"]
                | [
                    "v1",
                    "requests" | "events" | "sessions" | "agents" | "workspaces",
                    ..,
                ]
                | ["v1", "panes", _, "prompt" | "keys" | "interrupt" | "screen"],
            ) => {
                return Response::json(405, &json!({ "error": "method not allowed" }));
            }
            _ => return Response::json(404, &json!({ "error": "not found" })),
        };
        result.unwrap_or_else(|error| Response::error(status_for(&error), &error))
    }

    fn notify(&self, kind: &EventKind, origin: &Origin) {
        let Some(notifier) = &self.config.notifier else {
            return;
        };
        if let Some(notice) = notice_for(kind, origin, notifier.details) {
            notifier.send(notice);
        }
    }

    fn claude_hook(&self, request: &Request) -> Result<Response> {
        let Hook {
            origin,
            transcript_path,
            action,
        } = Hook::parse(&request.body, request.header("x-herdr-pane"))?;
        match action {
            Action::Record(kind) => {
                self.notify(&kind, &origin);
                self.broker.record(origin.clone(), kind);
                self.broker.touch_session(&origin, transcript_path);
                Ok(Response::empty())
            }
            Action::Permission {
                tool_name,
                tool_input,
                tool_use_id,
            } => {
                let summary = tool_summary(&tool_name, &tool_input);
                let notice_tool = tool_name.clone();
                let id = match self
                    .broker
                    .open(origin.clone(), tool_name, tool_input, tool_use_id)
                {
                    Ok(id) => id,
                    // Too many waiting: let the terminal prompt as usual.
                    Err(Error::PendingFull) => return Ok(Response::empty()),
                    Err(error) => return Err(error),
                };
                self.broker.touch_session(&origin, transcript_path);
                let kind = EventKind::PermissionRequested {
                    request_id: id,
                    tool_name: notice_tool,
                    summary,
                };
                self.notify(&kind, &origin);
                let output = self
                    .broker
                    .await_decision(id, self.config.decision_timeout)
                    .and_then(|(request, decision)| hook::permission_output(&request, decision));
                Ok(output.map_or_else(Response::empty, |output| Response::json(200, &output)))
            }
        }
    }

    fn decide(&self, id: &str, request: &Request) -> Result<Response> {
        let Ok(id) = id.parse() else {
            return Ok(Response::json(404, &json!({ "error": "not found" })));
        };
        let decision: Decision = serde_json::from_slice(&request.body)?;
        self.broker.decide(id, decision)?;
        Ok(Response::json(200, &json!({ "ok": true })))
    }

    fn events(&self, request: &Request) -> Response {
        let after = request
            .query("after")
            .and_then(|after| after.parse().ok())
            .unwrap_or(0);
        let wait = request
            .query("wait")
            .and_then(|wait| wait.parse().ok())
            .map_or(Duration::ZERO, Duration::from_secs)
            .min(MAX_EVENT_WAIT);
        Response::json(200, &json!(self.broker.events_after(after, wait)))
    }

    fn transcript(&self, session_id: &str, request: &Request) -> Result<Response> {
        let limit = request
            .query("limit")
            .and_then(|limit| limit.parse().ok())
            .unwrap_or(transcript::DEFAULT_ENTRIES);
        let path = self
            .broker
            .session(session_id)
            .and_then(|session| session.transcript_path)
            .ok_or(Error::NoTranscript)?;
        let entries = transcript::read(&path, limit)?;
        Ok(Response::json(200, &json!({ "entries": entries })))
    }

    fn agents(&self) -> Result<Response> {
        let herdr = self.herdr()?;
        let agents = herdr.agents()?;
        let labels: HashMap<String, String> = herdr
            .workspaces()?
            .into_iter()
            .map(|workspace| (workspace.workspace_id, workspace.label))
            .collect();
        let pending = self.broker.pending();
        let sessions = self.broker.sessions();
        let views: Vec<AgentView> = agents
            .into_iter()
            .map(|mut agent| {
                // Herdr knows Claude's session when its own hook reported it;
                // otherwise the most recent session our hooks saw in the pane.
                if agent.session_id.is_none() {
                    agent.session_id = sessions
                        .iter()
                        .find(|session| session.pane_id.as_deref() == Some(agent.pane_id.as_str()))
                        .map(|session| session.session_id.clone());
                }
                let has_transcript = agent.session_id.as_deref().is_some_and(|id| {
                    sessions
                        .iter()
                        .any(|session| session.session_id == id && session.has_transcript)
                });
                AgentView {
                    workspace_label: labels.get(&agent.workspace_id).cloned(),
                    pending: pending
                        .iter()
                        .filter(|request| {
                            request.origin.pane_id.as_deref() == Some(agent.pane_id.as_str())
                        })
                        .count(),
                    has_transcript,
                    agent,
                }
            })
            .collect();
        Ok(Response::json(200, &json!({ "agents": views })))
    }

    fn start(&self, request: &Request) -> Result<Response> {
        let start: StartAgent = serde_json::from_slice(&request.body)?;
        let agent = self.herdr()?.start(&start)?;
        Ok(Response::json(200, &json!({ "agent": agent })))
    }
}

fn status_for(error: &Error) -> u16 {
    match error {
        Error::UnknownRequest(_) | Error::NoTranscript => 404,
        Error::NotAQuestion(_) => 409,
        Error::PendingFull => 503,
        Error::Http(crate::HttpError::BodyTooLarge | crate::HttpError::HeadTooLarge) => 413,
        Error::Json(_)
        | Error::Http(_)
        | Error::EmptyDenyMessage
        | Error::EmptyPrompt
        | Error::KeyCount
        | Error::KeyNotAllowed(_) => 400,
        // Herdr understood and refused, for example an agent name in use.
        Error::Herdr { .. } => 422,
        Error::NoHerdrSocket
        | Error::HerdrClosed
        | Error::HerdrReplyTooLarge
        | Error::HerdrReplyShape
        | Error::Io(_) => 502,
        // Pairing happens in the CLI; no route renders a QR code.
        Error::Qr(_) => 500,
    }
}

/// Compares the whole token regardless of where the first difference is.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && left.iter().zip(right).fold(0, |acc, (a, b)| acc | (a ^ b)) == 0
}

/// Accepts connections until the listener fails. Each connection carries one
/// request; a hook's connection stays open until its decision or timeout.
pub fn serve(listener: TcpListener, companion: Arc<Companion>) -> Result<()> {
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if active.fetch_add(1, Ordering::AcqRel) >= companion.config.max_connections {
            active.fetch_sub(1, Ordering::AcqRel);
            let busy = Response::json(503, &json!({ "error": "too many connections" }));
            let _ = stream
                .set_write_timeout(Some(IO_TIMEOUT))
                .and_then(|()| busy.write_to(&mut &stream));
            continue;
        }
        let companion = Arc::clone(&companion);
        let active = Arc::clone(&active);
        thread::spawn(move || {
            // Diagnostics for a failed connection belong to that client; the
            // server keeps serving others either way.
            let _ = connection(&companion, stream);
            active.fetch_sub(1, Ordering::AcqRel);
        });
    }
    Ok(())
}

fn connection(companion: &Companion, stream: TcpStream) -> Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let response = match http::read_request(&mut BufReader::new(&stream)) {
        Ok(request) => companion.handle(&request),
        Err(error) => Response::error(status_for(&error), &error),
    };
    response.write_to(&mut &stream)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::broker::ASK_USER_QUESTION;
    use serde_json::Value;
    use std::io::{Read, Write};

    const TOKEN: &str = "test-token-0123456789abcdef";

    fn companion(decision_timeout: Duration) -> Arc<Companion> {
        Arc::new(Companion::new(Config {
            token: TOKEN.into(),
            decision_timeout,
            herdr_socket: None,
            notifier: None,
            limits: Limits::default(),
            max_connections: 8,
        }))
    }

    fn call(companion: &Companion, method: Method, target: &str, body: &Value) -> (u16, Value) {
        let auth = format!("Bearer {TOKEN}");
        let request = Request::new(
            method,
            target,
            &[("Authorization", &auth), ("X-Herdr-Pane", "p_2")],
            body.to_string().as_bytes(),
        );
        let response = companion.handle(&request);
        let body = if response.body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&response.body).unwrap()
        };
        (response.status, body)
    }

    fn permission(tool_name: &str, tool_input: Value) -> Value {
        json!({
            "session_id": "s1", "cwd": "/w", "hook_event_name": "PermissionRequest",
            "tool_name": tool_name, "tool_input": tool_input
        })
    }

    /// Runs the hook call on its own thread, as Claude Code's connection would,
    /// and returns the request id once the phone can see it.
    fn hook_in_background(
        companion: &Arc<Companion>,
        body: Value,
    ) -> (u64, thread::JoinHandle<(u16, Value)>) {
        let hook = {
            let companion = Arc::clone(companion);
            thread::spawn(move || call(&companion, Method::Post, "/hooks/claude", &body))
        };
        loop {
            if let Some(request) = companion.broker().pending().first() {
                return (request.id, hook);
            }
            thread::yield_now();
        }
    }

    #[test]
    fn every_route_requires_the_token() {
        let companion = companion(Duration::ZERO);
        for (auth, status) in [(None, 401), (Some("Bearer wrong"), 401), (Some(TOKEN), 401)] {
            let headers: Vec<(&str, &str)> = auth
                .map(|auth| ("Authorization", auth))
                .into_iter()
                .collect();
            let response =
                companion.handle(&Request::new(Method::Get, "/v1/requests", &headers, b""));
            assert_eq!(response.status, status, "{auth:?}");
        }
        assert_eq!(
            call(&companion, Method::Get, "/v1/requests", &Value::Null).0,
            200
        );
    }

    #[test]
    fn a_phone_approval_reaches_the_waiting_hook() {
        let companion = companion(Duration::from_secs(30));
        let (id, hook) =
            hook_in_background(&companion, permission("Bash", json!({"command": "ls"})));

        let (status, listed) = call(&companion, Method::Get, "/v1/requests", &Value::Null);
        assert_eq!(status, 200);
        assert_eq!(listed["requests"][0]["pane_id"], "p_2");
        assert_eq!(listed["requests"][0]["tool_input"]["command"], "ls");

        let (status, _) = call(
            &companion,
            Method::Post,
            &format!("/v1/requests/{id}"),
            &json!({"behavior": "allow"}),
        );
        assert_eq!(status, 200);
        let (status, output) = hook.join().unwrap();
        assert_eq!(status, 200);
        assert_eq!(
            output["hookSpecificOutput"]["decision"],
            json!({"behavior": "allow", "updatedInput": {"command": "ls"}})
        );
    }

    #[test]
    fn a_question_is_answered_from_the_phone() {
        let companion = companion(Duration::from_secs(30));
        let questions = json!([{"question": "Which DB?", "header": "DB", "options": [{"label": "Postgres", "description": ""}], "multiSelect": false}]);
        let (id, hook) = hook_in_background(
            &companion,
            permission(ASK_USER_QUESTION, json!({"questions": questions})),
        );
        let (status, _) = call(
            &companion,
            Method::Post,
            &format!("/v1/requests/{id}"),
            &json!({"behavior": "answer", "answers": {"Which DB?": "Postgres"}}),
        );
        assert_eq!(status, 200);
        let (_, output) = hook.join().unwrap();
        assert_eq!(
            output["hookSpecificOutput"]["decision"]["updatedInput"]["answers"]["Which DB?"],
            "Postgres"
        );
    }

    #[test]
    fn an_unanswered_prompt_falls_back_to_the_terminal() {
        let companion = companion(Duration::ZERO);
        let (status, body) = call(
            &companion,
            Method::Post,
            "/hooks/claude",
            &permission("Bash", json!({})),
        );
        assert_eq!(
            (status, body),
            (200, Value::Null),
            "an empty 200 is no decision"
        );
        let (_, events) = call(&companion, Method::Get, "/v1/events?after=0", &Value::Null);
        assert_eq!(events["events"][1]["outcome"], "timed_out");
    }

    #[test]
    fn events_are_recorded_and_paged() {
        let companion = companion(Duration::ZERO);
        let stop = json!({"session_id": "s1", "hook_event_name": "Stop", "last_assistant_message": "done"});
        assert_eq!(
            call(&companion, Method::Post, "/hooks/claude", &stop),
            (200, Value::Null)
        );
        let (status, page) = call(
            &companion,
            Method::Get,
            "/v1/events?after=0&wait=0",
            &Value::Null,
        );
        assert_eq!(status, 200);
        assert_eq!(page["next"], 1);
        assert_eq!(page["events"][0]["kind"], "stopped");
        assert_eq!(page["events"][0]["last_assistant_message"], "done");
        assert_eq!(page["events"][0]["pane_id"], "p_2");
    }

    #[test]
    fn bad_input_maps_to_client_errors() {
        let companion = companion(Duration::ZERO);
        assert_eq!(
            call(
                &companion,
                Method::Post,
                "/v1/requests/9",
                &json!({"behavior": "terminal"})
            )
            .0,
            404
        );
        assert_eq!(
            call(
                &companion,
                Method::Post,
                "/v1/requests/x",
                &json!({"behavior": "terminal"})
            )
            .0,
            404
        );
        assert_eq!(
            call(
                &companion,
                Method::Post,
                "/v1/requests/1",
                &json!({"behavior": "nope"})
            )
            .0,
            400
        );
        assert_eq!(
            call(
                &companion,
                Method::Post,
                "/hooks/claude",
                &json!({"no": "event"})
            )
            .0,
            400
        );
        assert_eq!(
            call(&companion, Method::Get, "/hooks/claude", &Value::Null).0,
            405
        );
        assert_eq!(call(&companion, Method::Get, "/nope", &Value::Null).0, 404);
        assert_eq!(
            call(
                &companion,
                Method::Post,
                "/v1/panes/p_1/prompt",
                &json!({"text": "hi"})
            )
            .0,
            502
        );
    }

    #[test]
    fn serves_a_hook_over_a_real_socket() {
        let companion = companion(Duration::ZERO);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || serve(listener, companion));

        let body = json!({"session_id": "s", "hook_event_name": "Notification", "message": "hi"})
            .to_string();
        let mut stream = TcpStream::connect(address).unwrap();
        write!(
            stream,
            "POST /hooks/claude HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {TOKEN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 200 OK\r\n"), "{reply}");
        assert!(reply.contains("Content-Length: 0\r\n"));
    }

    #[cfg(unix)]
    fn with_herdr(socket: PathBuf) -> Arc<Companion> {
        Arc::new(Companion::new(Config {
            token: TOKEN.into(),
            decision_timeout: Duration::ZERO,
            herdr_socket: Some(socket),
            notifier: None,
            limits: Limits::default(),
            max_connections: 8,
        }))
    }

    #[test]
    fn the_web_app_is_public_and_the_api_is_not() {
        let companion = companion(Duration::ZERO);
        let response = companion.handle(&Request::new(Method::Get, "/", &[], b""));
        assert_eq!(response.status, 200);
        assert!(response.content_type.starts_with("text/html"));
        let response = companion.handle(&Request::new(Method::Get, "/app.js", &[], b""));
        assert!(response.content_type.starts_with("text/javascript"));
        // Assets answer GET only; anything else still needs the token.
        let response = companion.handle(&Request::new(Method::Post, "/", &[], b""));
        assert_eq!(response.status, 401);
        let response = companion.handle(&Request::new(Method::Get, "/v1/agents", &[], b""));
        assert_eq!(response.status, 401);
    }

    #[test]
    fn a_session_reads_as_chat_from_its_transcript() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s1.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\n{}\n",
                json!({"type": "user", "message": {"content": "hello"}}),
                json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "hi"}]}})
            ),
        )
        .unwrap();
        let companion = companion(Duration::ZERO);
        assert_eq!(
            call(
                &companion,
                Method::Get,
                "/v1/sessions/s1/transcript",
                &Value::Null
            )
            .0,
            404
        );
        let start = json!({"session_id": "s1", "hook_event_name": "SessionStart", "cwd": "/w", "transcript_path": path});
        assert_eq!(
            call(&companion, Method::Post, "/hooks/claude", &start).0,
            200
        );

        let (status, sessions) = call(&companion, Method::Get, "/v1/sessions", &Value::Null);
        assert_eq!(status, 200);
        assert_eq!(sessions["sessions"][0]["session_id"], "s1");
        assert_eq!(sessions["sessions"][0]["pane_id"], "p_2");
        assert_eq!(sessions["sessions"][0]["has_transcript"], true);
        assert!(
            sessions["sessions"][0].get("transcript_path").is_none(),
            "paths stay server-side"
        );

        let (status, chat) = call(
            &companion,
            Method::Get,
            "/v1/sessions/s1/transcript?limit=1",
            &Value::Null,
        );
        assert_eq!(status, 200);
        assert_eq!(
            chat["entries"],
            json!([{"kind": "assistant", "text": "hi", "at": null}])
        );
    }

    #[test]
    fn tool_hooks_show_progress_in_the_feed() {
        let companion = companion(Duration::ZERO);
        let pre = json!({"session_id": "s1", "hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": "cargo test"}});
        assert_eq!(
            call(&companion, Method::Post, "/hooks/claude", &pre),
            (200, Value::Null)
        );
        let (_, page) = call(&companion, Method::Get, "/v1/events", &Value::Null);
        assert_eq!(page["events"][0]["kind"], "tool_started");
        assert_eq!(page["events"][0]["summary"], "cargo test");
    }

    #[test]
    fn herdr_routes_need_a_socket() {
        let companion = companion(Duration::ZERO);
        for (method, target) in [
            (Method::Get, "/v1/agents"),
            (Method::Get, "/v1/workspaces"),
            (Method::Get, "/v1/panes/p/screen"),
            (Method::Post, "/v1/panes/p/interrupt"),
        ] {
            assert_eq!(
                call(&companion, method, target, &json!({})).0,
                502,
                "{target}"
            );
        }
    }

    /// A stand-in for Herdr's JSON API: answers each request from `reply`
    /// and records every method and its params.
    #[cfg(unix)]
    struct FakeHerdr {
        _dir: tempfile::TempDir,
        socket: PathBuf,
        calls: Arc<std::sync::Mutex<Vec<(String, Value)>>>,
    }

    #[cfg(unix)]
    impl FakeHerdr {
        fn start(reply: impl Fn(&str, &Value) -> Value + Send + 'static) -> Self {
            use std::{io::BufRead, os::unix::net::UnixListener};
            let dir = tempfile::tempdir().unwrap();
            let socket = dir.path().join("herdr.sock");
            let listener = UnixListener::bind(&socket).unwrap();
            let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
            let seen = Arc::clone(&calls);
            thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { return };
                    let mut line = String::new();
                    if BufReader::new(&stream).read_line(&mut line).is_err() {
                        continue;
                    }
                    let request: Value = serde_json::from_str(&line).unwrap();
                    let method = request["method"].as_str().unwrap_or_default().to_owned();
                    let response = reply(&method, &request["params"]);
                    seen.lock()
                        .unwrap()
                        .push((method, request["params"].clone()));
                    let _ = writeln!(
                        stream,
                        "{}",
                        json!({"id": request["id"], "result": response})
                    );
                }
            });
            Self {
                _dir: dir,
                socket,
                calls,
            }
        }

        fn calls(&self) -> Vec<(String, Value)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[cfg(unix)]
    fn herdr_reply(method: &str, params: &Value) -> Value {
        match method {
            "agent.list" => json!({"type": "agent_list", "agents": [
                {"pane_id": "p_2", "workspace_id": "w1", "tab_id": "t1", "terminal_id": "x", "revision": 1,
                 "focused": false, "agent": "claude", "agent_status": "blocked", "cwd": "/w"},
                {"pane_id": "p_9", "workspace_id": "w2", "tab_id": "t2", "terminal_id": "y", "revision": 1,
                 "focused": true, "agent": "codex", "agent_status": "working"}
            ]}),
            "workspace.list" => json!({"type": "workspace_list", "workspaces": [
                {"workspace_id": "w1", "number": 1, "label": "api", "focused": false, "pane_count": 1,
                 "tab_count": 1, "active_tab_id": "t1", "agent_status": "blocked"}
            ]}),
            "pane.read" => {
                json!({"type": "pane_read", "read": {"pane_id": params["pane_id"], "text": "Trust this folder? 1. Yes"}})
            }
            "tab.create" => {
                json!({"type": "tab_created", "tab": {}, "root_pane": {"pane_id": "p_new"}})
            }
            "agent.start" => json!({"type": "agent_started", "argv": ["claude"], "agent": {
                "pane_id": "p_new", "workspace_id": "w1", "tab_id": "t3", "terminal_id": "z", "revision": 1,
                "focused": false, "name": "api-fix", "agent": "claude", "agent_status": "idle"}}),
            _ => json!({"type": "ok"}),
        }
    }

    #[cfg(unix)]
    #[test]
    fn agents_combine_herdr_with_what_the_hooks_know() {
        let herdr = FakeHerdr::start(herdr_reply);
        let companion = with_herdr(herdr.socket.clone());
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("s1.jsonl");
        let start = json!({"session_id": "s1", "hook_event_name": "SessionStart", "transcript_path": transcript});
        call(&companion, Method::Post, "/hooks/claude", &start);

        let (status, body) = call(&companion, Method::Get, "/v1/agents", &Value::Null);
        assert_eq!(status, 200, "{body}");
        let agents = body["agents"].as_array().unwrap();
        assert_eq!(agents[0]["pane_id"], "p_2");
        assert_eq!(agents[0]["workspace_label"], "api");
        assert_eq!(
            agents[0]["session_id"], "s1",
            "learned from the hook in that pane"
        );
        assert_eq!(agents[0]["has_transcript"], true);
        assert_eq!(agents[0]["pending"], 0);
        assert_eq!(agents[1]["workspace_label"], Value::Null);
        assert_eq!(agents[1]["session_id"], Value::Null);
    }

    #[cfg(unix)]
    #[test]
    fn pending_prompts_are_counted_per_agent() {
        let herdr = FakeHerdr::start(herdr_reply);
        let companion = Arc::new(Companion::new(Config {
            token: TOKEN.into(),
            decision_timeout: Duration::from_secs(30),
            herdr_socket: Some(herdr.socket.clone()),
            notifier: None,
            limits: Limits::default(),
            max_connections: 8,
        }));
        let (id, hook) =
            hook_in_background(&companion, permission("Bash", json!({"command": "ls"})));
        let (_, body) = call(&companion, Method::Get, "/v1/agents", &Value::Null);
        assert_eq!(body["agents"][0]["pending"], 1);
        assert_eq!(body["agents"][1]["pending"], 0);
        call(
            &companion,
            Method::Post,
            &format!("/v1/requests/{id}"),
            &json!({"behavior": "terminal"}),
        );
        assert_eq!(hook.join().unwrap(), (200, Value::Null));
    }

    #[cfg(unix)]
    #[test]
    fn panes_take_prompts_keys_interrupts_and_screen_reads() {
        let herdr = FakeHerdr::start(herdr_reply);
        let companion = with_herdr(herdr.socket.clone());
        assert_eq!(
            call(
                &companion,
                Method::Post,
                "/v1/panes/p_2/prompt",
                &json!({"text": "run the tests"})
            )
            .0,
            200
        );
        assert_eq!(
            call(
                &companion,
                Method::Post,
                "/v1/panes/p_2/keys",
                &json!({"keys": ["down", "enter"]})
            )
            .0,
            200
        );
        assert_eq!(
            call(
                &companion,
                Method::Post,
                "/v1/panes/p_2/keys",
                &json!({"keys": ["ctrl+d"]})
            )
            .0,
            400
        );
        assert_eq!(
            call(
                &companion,
                Method::Post,
                "/v1/panes/p_2/interrupt",
                &Value::Null
            )
            .0,
            200
        );
        let (status, screen) = call(
            &companion,
            Method::Get,
            "/v1/panes/p_2/screen",
            &Value::Null,
        );
        assert_eq!(
            (status, &screen["text"]),
            (200, &json!("Trust this folder? 1. Yes"))
        );
        assert_eq!(
            herdr.calls(),
            [
                (
                    "agent.prompt".to_owned(),
                    json!({"target": "p_2", "text": "run the tests"})
                ),
                (
                    "pane.send_keys".to_owned(),
                    json!({"pane_id": "p_2", "keys": ["down", "enter"]})
                ),
                (
                    "pane.send_keys".to_owned(),
                    json!({"pane_id": "p_2", "keys": ["esc"]})
                ),
                (
                    "pane.read".to_owned(),
                    json!({"pane_id": "p_2", "source": "visible", "format": "text", "strip_ansi": true})
                ),
            ],
            "the refused key never reached Herdr"
        );
    }

    #[cfg(unix)]
    #[test]
    fn starting_an_agent_creates_its_pane_first() {
        let herdr = FakeHerdr::start(herdr_reply);
        let companion = with_herdr(herdr.socket.clone());
        let body = json!({"kind": "claude", "name": "api-fix", "placement": {"in": "tab", "workspace_id": "w1", "cwd": "/w"}});
        let (status, started) = call(&companion, Method::Post, "/v1/agents", &body);
        assert_eq!(status, 200, "{started}");
        assert_eq!(started["agent"]["pane_id"], "p_new");
        let calls = herdr.calls();
        assert_eq!(
            calls[0],
            (
                "tab.create".to_owned(),
                json!({"workspace_id": "w1", "cwd": "/w", "label": "api-fix"})
            )
        );
        assert_eq!(calls[1].0, "agent.start");
        assert_eq!(calls[1].1["pane_id"], "p_new");
        assert_eq!(calls[1].1["kind"], "claude");
    }

    #[cfg(unix)]
    #[test]
    fn a_herdr_refusal_is_a_client_error() {
        use std::{io::BufRead, os::unix::net::UnixListener};
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("herdr.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut line = String::new();
                let _ = BufReader::new(&stream).read_line(&mut line);
                let _ = writeln!(
                    stream,
                    r#"{{"id":"x","error":{{"code":"agent_blocked","message":"agent is blocked"}}}}"#
                );
            }
        });
        let companion = with_herdr(socket);
        let (status, body) = call(
            &companion,
            Method::Post,
            "/v1/panes/p_2/prompt",
            &json!({"text": "hi"}),
        );
        assert_eq!(status, 422);
        assert!(body["error"].as_str().unwrap().contains("agent_blocked"));
    }
}
