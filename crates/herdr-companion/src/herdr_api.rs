//! Calls into Herdr's newline-delimited JSON API socket (`herdr.sock`), not the
//! binary client socket the GUI attaches to. Every call opens one connection,
//! sends one request, and reads one bounded reply. Replies to the agent go
//! through `agent.prompt`; keys are reserved for screens no API method covers.

use crate::{Error, Result};
use herdr_client::{ConnectTarget, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    env,
    ffi::OsString,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(10);
/// Herdr waits for a started agent to become interactive; its own default
/// startup timeout is 30 s, so the reply needs longer than an ordinary call.
const START_TIMEOUT_MS: u64 = 30_000;
const START_READ_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_REPLY: u64 = 1024 * 1024;
const MAX_KEYS: usize = 16;
/// Keys a phone may send to a pane, for dialogs that no API method answers
/// (trust prompts, logins, menus). Anything that edits text goes through
/// `agent.prompt` instead.
const ALLOWED_KEYS: &[&str] = &[
    "enter",
    "esc",
    "tab",
    "space",
    "backspace",
    "up",
    "down",
    "left",
    "right",
    "ctrl+c",
    "y",
    "n",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
];

/// The API socket a hook-launched process would use: `HERDR_SOCKET_PATH`,
/// else `herdr.sock` beside the local session's client socket.
pub fn default_socket() -> Result<PathBuf> {
    default_socket_with(|name| env::var_os(name))
}

fn default_socket_with(var: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
    if let Some(path) = var("HERDR_SOCKET_PATH") {
        return Ok(path.into());
    }
    let client = ConnectTarget::Local
        .local_session_socket_path()
        .map_err(|_| Error::NoHerdrSocket)?;
    Ok(client.with_file_name("herdr.sock"))
}

/// One agent as Herdr reports it. Only the fields a phone shows are kept.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Agent {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub display_agent: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    pub agent_status: AgentStatus,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub state_labels: Map<String, Value>,
    /// The agent's own session id when its integration reported one; for
    /// Claude Code it matches the hooks' `session_id`.
    #[serde(
        default,
        rename(deserialize = "agent_session"),
        deserialize_with = "session_value"
    )]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    Blocked,
    Done,
    #[serde(other)]
    Unknown,
}

fn session_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    struct Session {
        value: String,
    }
    Ok(Option::<Session>::deserialize(deserializer)?.map(|session| session.value))
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Workspace {
    pub workspace_id: String,
    pub label: String,
    #[serde(default)]
    pub focused: bool,
    pub agent_status: AgentStatus,
}

/// Where a new agent's pane is created.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "in", rename_all = "snake_case")]
pub enum Placement {
    /// A new tab in an existing workspace, optionally in a given directory.
    Tab {
        workspace_id: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
    },
    /// A new Git worktree on `branch`, created from the repository at `cwd`.
    Worktree { cwd: String, branch: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StartAgent {
    /// Herdr's agent kind, such as `claude` or `codex`.
    pub kind: String,
    /// Herdr's unique agent name, `[a-z][a-z0-9_-]{0,31}`.
    pub name: String,
    pub placement: Placement,
}

/// Herdr's JSON API socket.
pub struct Herdr<'a> {
    socket: &'a Path,
}

impl<'a> Herdr<'a> {
    pub fn new(socket: &'a Path) -> Self {
        Self { socket }
    }

    fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let mut stream = Stream::connect(self.socket)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(TIMEOUT))?;
        exchange(&mut stream, &request(method, params))
    }

    pub fn agents(&self) -> Result<Vec<Agent>> {
        list(self.call("agent.list", json!({}), TIMEOUT)?, "agents")
    }

    pub fn workspaces(&self) -> Result<Vec<Workspace>> {
        list(
            self.call("workspace.list", json!({}), TIMEOUT)?,
            "workspaces",
        )
    }

    pub fn prompt(&self, pane_id: &str, text: &str) -> Result<()> {
        if text.trim().is_empty() {
            return Err(Error::EmptyPrompt);
        }
        let params = json!({ "target": pane_id, "text": text });
        self.call("agent.prompt", params, TIMEOUT).map(drop)
    }

    /// The pane's visible screen as plain text.
    pub fn screen(&self, pane_id: &str) -> Result<String> {
        let params = json!({ "pane_id": pane_id, "source": "visible", "format": "text", "strip_ansi": true });
        let mut result = self.call("pane.read", params, TIMEOUT)?;
        match result.pointer_mut("/read/text").map(Value::take) {
            Some(Value::String(text)) => Ok(text),
            _ => Err(Error::HerdrReplyShape),
        }
    }

    pub fn send_keys(&self, pane_id: &str, keys: &[String]) -> Result<()> {
        check_keys(keys)?;
        let params = json!({ "pane_id": pane_id, "keys": keys });
        self.call("pane.send_keys", params, TIMEOUT).map(drop)
    }

    /// Creates the pane, then hands it to `agent.start`, which requires an
    /// idle shell pane and returns once the agent is ready for input.
    pub fn start(&self, start: &StartAgent) -> Result<Agent> {
        let created = match &start.placement {
            Placement::Tab { workspace_id, cwd } => {
                let params =
                    json!({ "workspace_id": workspace_id, "cwd": cwd, "label": start.name });
                self.call("tab.create", params, TIMEOUT)?
            }
            Placement::Worktree { cwd, branch } => {
                let params = json!({ "cwd": cwd, "branch": branch, "label": start.name });
                self.call("worktree.create", params, TIMEOUT)?
            }
        };
        let pane_id = created
            .pointer("/root_pane/pane_id")
            .and_then(Value::as_str)
            .ok_or(Error::HerdrReplyShape)?;
        let params = json!({
            "name": start.name,
            "kind": start.kind,
            "pane_id": pane_id,
            "timeout_ms": START_TIMEOUT_MS,
        });
        let mut started = self.call("agent.start", params, START_READ_TIMEOUT)?;
        let agent = started
            .get_mut("agent")
            .map(Value::take)
            .ok_or(Error::HerdrReplyShape)?;
        Ok(serde_json::from_value(agent)?)
    }
}

fn check_keys(keys: &[String]) -> Result<()> {
    if keys.is_empty() || keys.len() > MAX_KEYS {
        return Err(Error::KeyCount);
    }
    match keys
        .iter()
        .find(|key| !ALLOWED_KEYS.contains(&key.as_str()))
    {
        Some(key) => Err(Error::KeyNotAllowed(key.clone())),
        None => Ok(()),
    }
}

fn request(method: &str, params: Value) -> Value {
    json!({ "id": "herdr-companion", "method": method, "params": params })
}

fn list<T: for<'de> Deserialize<'de>>(mut result: Value, field: &str) -> Result<Vec<T>> {
    let items = result
        .get_mut(field)
        .map(Value::take)
        .ok_or(Error::HerdrReplyShape)?;
    Ok(serde_json::from_value(items)?)
}

fn exchange(stream: &mut (impl Read + Write), request: &Value) -> Result<Value> {
    let mut line = request.to_string().into_bytes();
    line.push(b'\n');
    stream.write_all(&line)?;
    stream.flush()?;

    let mut reply = Vec::new();
    BufReader::new(stream.take(MAX_REPLY)).read_until(b'\n', &mut reply)?;
    if reply.last() != Some(&b'\n') {
        return Err(if reply.len() as u64 == MAX_REPLY {
            Error::HerdrReplyTooLarge
        } else {
            Error::HerdrClosed
        });
    }
    let mut reply: Value = serde_json::from_slice(&reply)?;
    if let Some(error) = reply.get("error") {
        let field = |name| {
            error
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        return Err(Error::Herdr {
            code: field("code"),
            message: field("message"),
        });
    }
    Ok(reply
        .get_mut("result")
        .map(Value::take)
        .unwrap_or(Value::Null))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};

    /// Replays a canned reply and keeps what was written.
    struct Peer {
        reply: Cursor<Vec<u8>>,
        written: Vec<u8>,
    }

    impl Peer {
        fn new(reply: &[u8]) -> Self {
            Self {
                reply: Cursor::new(reply.to_vec()),
                written: Vec::new(),
            }
        }
    }

    impl Read for Peer {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.reply.read(buf)
        }
    }

    impl Write for Peer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.written.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn sends_one_json_line_and_returns_the_result() {
        let mut peer =
            Peer::new(b"{\"id\":\"herdr-companion\",\"result\":{\"type\":\"agent_prompted\"}}\n");
        let request = request("agent.prompt", json!({"target": "p"}));
        let result = exchange(&mut peer, &request).unwrap();
        assert_eq!(result, json!({"type": "agent_prompted"}));
        assert_eq!(peer.written, format!("{request}\n").into_bytes());
    }

    #[test]
    fn surfaces_the_daemons_error_fields() {
        let mut peer = Peer::new(
            b"{\"id\":\"x\",\"error\":{\"code\":\"agent_not_found\",\"message\":\"no agent\"}}\n",
        );
        let error = exchange(&mut peer, &json!({})).unwrap_err();
        assert!(matches!(
            error,
            Error::Herdr { code, message } if code == "agent_not_found" && message == "no agent"
        ));
    }

    #[test]
    fn a_closed_or_oversized_reply_is_an_error() {
        assert!(matches!(
            exchange(&mut Peer::new(b"{\"id\""), &json!({})),
            Err(Error::HerdrClosed)
        ));
        let huge = vec![b'x'; MAX_REPLY as usize + 1];
        assert!(matches!(
            exchange(&mut Peer::new(&huge), &json!({})),
            Err(Error::HerdrReplyTooLarge)
        ));
    }

    #[test]
    fn decodes_agents_with_their_session_and_unknown_statuses() {
        let result = json!({"type": "agent_list", "agents": [
            {"pane_id": "w1-p1", "workspace_id": "w1", "tab_id": "w1-t1", "terminal_id": "t", "revision": 3,
             "focused": true, "agent": "claude", "display_agent": "Claude Code", "agent_status": "blocked",
             "cwd": "/w", "state_labels": {"blocked": "needs you"},
             "agent_session": {"source": "claude-hook", "agent": "claude", "kind": "session", "value": "abc"}},
            {"pane_id": "w1-p2", "workspace_id": "w1", "tab_id": "w1-t1", "terminal_id": "u", "revision": 1,
             "focused": false, "agent_status": "napping"}
        ]});
        let agents: Vec<Agent> = list(result, "agents").unwrap();
        assert_eq!(agents[0].agent_status, AgentStatus::Blocked);
        assert_eq!(agents[0].session_id.as_deref(), Some("abc"));
        assert_eq!(agents[0].state_labels["blocked"], "needs you");
        assert_eq!(agents[1].agent_status, AgentStatus::Unknown);
        assert_eq!(agents[1].session_id, None);
        assert!(matches!(
            list::<Agent>(json!({}), "agents"),
            Err(Error::HerdrReplyShape)
        ));
    }

    #[test]
    fn decodes_a_placement_from_the_phone() {
        let start: StartAgent = serde_json::from_value(json!({
            "kind": "claude", "name": "api", "placement": {"in": "worktree", "cwd": "/repo", "branch": "fix"}
        }))
        .unwrap();
        assert_eq!(
            start.placement,
            Placement::Worktree {
                cwd: "/repo".into(),
                branch: "fix".into()
            }
        );
        let tab: Placement =
            serde_json::from_value(json!({"in": "tab", "workspace_id": "w2"})).unwrap();
        assert_eq!(
            tab,
            Placement::Tab {
                workspace_id: Some("w2".into()),
                cwd: None
            }
        );
    }

    #[test]
    fn only_allowlisted_keys_in_bounded_batches() {
        let keys = |list: &[&str]| list.iter().map(|key| (*key).to_owned()).collect::<Vec<_>>();
        check_keys(&keys(&["esc", "down", "enter", "ctrl+c", "2"])).unwrap();
        assert!(
            matches!(check_keys(&keys(&["ctrl+d"])), Err(Error::KeyNotAllowed(key)) if key == "ctrl+d")
        );
        assert!(matches!(check_keys(&[]), Err(Error::KeyCount)));
        assert!(matches!(
            check_keys(&keys(&["y"; MAX_KEYS + 1])),
            Err(Error::KeyCount)
        ));
    }

    #[test]
    fn prefers_the_explicit_api_socket() {
        let path =
            default_socket_with(|name| (name == "HERDR_SOCKET_PATH").then(|| "/run/h.sock".into()))
                .unwrap();
        assert_eq!(path, PathBuf::from("/run/h.sock"));
    }

    #[test]
    fn refuses_bad_input_before_connecting() {
        let herdr = Herdr::new(Path::new("/nonexistent"));
        assert!(matches!(herdr.prompt("p_1", "  "), Err(Error::EmptyPrompt)));
        assert!(matches!(
            herdr.send_keys("p_1", &["rm".into()]),
            Err(Error::KeyNotAllowed(_))
        ));
    }
}
