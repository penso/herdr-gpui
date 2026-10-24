//! Herdr's public API socket: newline-delimited JSON, one request per
//! connection. Its pane and tab methods neither focus nor resize anything
//! for any client, and it takes requests the inactive shell connection may
//! not send (Herdr answers those `surface_inactive`), so everything a phone
//! changes goes through here.

use super::{Error, Result};
use herdr_client::{ConnectTarget, Stream};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

/// How long a request waits for Herdr's answer.
const TIMEOUT: Duration = Duration::from_secs(15);
/// The longest answer read; a visible screen is far smaller.
const MAX_ANSWER: u64 = 4 * 1024 * 1024;

/// The methods the daemon calls, by their wire names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ApiMethod {
    // keep-sorted start
    PaneClose,
    PaneRead,
    PaneSendInput,
    PaneSendKeys,
    PaneSendText,
    PaneSplit,
    PaneZoom,
    TabClose,
    TabCreate,
    WorkspaceCreate,
    WorktreeCreate,
    // keep-sorted end
}

impl ApiMethod {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::PaneClose => "pane.close",
            Self::PaneRead => "pane.read",
            Self::PaneSendInput => "pane.send_input",
            Self::PaneSendKeys => "pane.send_keys",
            Self::PaneSendText => "pane.send_text",
            Self::PaneSplit => "pane.split",
            Self::PaneZoom => "pane.zoom",
            Self::TabClose => "tab.close",
            Self::TabCreate => "tab.create",
            Self::WorkspaceCreate => "workspace.create",
            Self::WorktreeCreate => "worktree.create",
        }
    }
}

/// Where the API socket is: beside the client socket the daemon follows.
#[derive(Clone, Debug)]
pub(crate) struct Api {
    /// The client socket Herdr UI resolved; `None` discovers it.
    client_socket: Option<PathBuf>,
}

impl Api {
    pub(crate) fn new(client_socket: Option<PathBuf>) -> Self {
        Self { client_socket }
    }

    fn path(&self) -> Result<PathBuf> {
        let client = match &self.client_socket {
            Some(path) => path.clone(),
            None => ConnectTarget::Local.socket_path()?,
        };
        api_socket(&client).ok_or(Error::NotConnected)
    }

    /// Calls `method` and returns its `result`. Runs on the blocking pool.
    pub(crate) async fn call(&self, method: ApiMethod, params: Value) -> Result<Value> {
        let path = self.path()?;
        tokio::task::spawn_blocking(move || call(&path, method, &params))
            .await
            .map_err(|_| Error::NotConnected)?
    }
}

/// The API socket a daemon serves beside `client`: `herdr.sock` beside
/// `herdr-client.sock`, `X.sock` beside `X-client.sock`.
pub(crate) fn api_socket(client: &Path) -> Option<PathBuf> {
    let name = client.file_name()?.to_str()?;
    let stem = name.strip_suffix("-client.sock")?;
    (!stem.is_empty()).then(|| client.with_file_name(format!("{stem}.sock")))
}

/// One request on its own connection. Blocking.
fn call(path: &Path, method: ApiMethod, params: &Value) -> Result<Value> {
    let mut stream = Stream::connect(path).map_err(|_| Error::NotConnected)?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|_| Error::NotConnected)?;
    let mut line = json!({"id": "remote", "method": method.name(), "params": params}).to_string();
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .and_then(|()| stream.flush())
        .map_err(|_| Error::NotConnected)?;
    let mut answer = String::new();
    BufReader::new((&mut stream).take(MAX_ANSWER))
        .read_line(&mut answer)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => Error::Timeout,
            _ => Error::NotConnected,
        })?;
    let answer: Value = serde_json::from_str(&answer).map_err(|_| Error::Response)?;
    result(answer)
}

/// The `result` of an answer, or its error with a bounded message.
pub(crate) fn result(answer: Value) -> Result<Value> {
    match answer.get("error").filter(|error| !error.is_null()) {
        Some(error) => Err(Error::Daemon(
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("the request failed")
                .chars()
                .take(200)
                .collect(),
        )),
        None => Ok(answer.get("result").cloned().unwrap_or(Value::Null)),
    }
}
