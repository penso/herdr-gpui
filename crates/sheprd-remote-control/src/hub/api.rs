//! Herdr's public API socket: newline-delimited JSON, one request per
//! connection. Its pane and tab methods neither focus nor resize anything
//! for any client, and it takes requests the inactive shell connection may
//! not send (Herdr answers those `surface_inactive`), so everything a phone
//! changes goes through here.

use super::{Error, Result};
use serde_json::{Value, json};
use sheprd_client::{ConnectTarget, Stream};
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
    PaneGet,
    PaneRead,
    PaneRename,
    PaneSendInput,
    PaneSendKeys,
    PaneSendText,
    PaneSplit,
    PaneZoom,
    TabClose,
    TabCreate,
    TabRename,
    WorkspaceCreate,
    WorkspaceRename,
    WorktreeCreate,
    // keep-sorted end
}

impl ApiMethod {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::PaneClose => "pane.close",
            Self::PaneGet => "pane.get",
            Self::PaneRead => "pane.read",
            Self::PaneRename => "pane.rename",
            Self::PaneSendInput => "pane.send_input",
            Self::PaneSendKeys => "pane.send_keys",
            Self::PaneSendText => "pane.send_text",
            Self::PaneSplit => "pane.split",
            Self::PaneZoom => "pane.zoom",
            Self::TabClose => "tab.close",
            Self::TabCreate => "tab.create",
            Self::TabRename => "tab.rename",
            Self::WorkspaceCreate => "workspace.create",
            Self::WorkspaceRename => "workspace.rename",
            Self::WorktreeCreate => "worktree.create",
        }
    }
}

/// Where the API socket is: beside the client socket the daemon follows, or
/// behind Herdr's API bridge in a WSL distribution.
#[derive(Clone, Debug)]
pub(crate) struct Api {
    target: ConnectTarget,
}

impl Api {
    /// The API of the daemon at `client_socket`; `None` discovers it.
    pub(crate) fn new(client_socket: Option<PathBuf>) -> Self {
        Self::at(client_socket.map_or(ConnectTarget::Local, ConnectTarget::Socket))
    }

    pub(crate) fn at(target: ConnectTarget) -> Self {
        Self { target }
    }

    /// Where a direct terminal attach connects.
    pub(crate) fn target(&self) -> ConnectTarget {
        self.target.clone()
    }

    /// Calls `method` and returns its `result`. Runs on the blocking pool.
    pub(crate) async fn call(&self, method: ApiMethod, params: Value) -> Result<Value> {
        let target = self.target.clone();
        tokio::task::spawn_blocking(move || call(&target, method, &params))
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
fn call(target: &ConnectTarget, method: ApiMethod, params: &Value) -> Result<Value> {
    if let ConnectTarget::Wsl { distro, session } = target {
        let mut bridge =
            sheprd_client::wsl_api_bridge(distro, session).map_err(|_| Error::NotConnected)?;
        return exchange(bridge.stream_mut(), method, params);
    }
    let path = api_socket(&target.socket_path()?).ok_or(Error::NotConnected)?;
    let mut stream = Stream::connect(path).map_err(|_| Error::NotConnected)?;
    exchange(&mut stream, method, params)
}

fn exchange(stream: &mut Stream, method: ApiMethod, params: &Value) -> Result<Value> {
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
    BufReader::new(stream.take(MAX_ANSWER))
        .read_line(&mut answer)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => Error::Timeout,
            _ => Error::NotConnected,
        })?;
    // A bridge that found no Herdr to run closes without a word.
    if answer.is_empty() {
        return Err(Error::NotConnected);
    }
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
