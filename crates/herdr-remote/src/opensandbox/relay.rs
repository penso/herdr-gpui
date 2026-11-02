//! This executable as an OpenSandbox sandbox's standard streams. Cloud
//! devices reach their machines by running a command whose stdin and stdout
//! are the remote shell's, the way `ssh host COMMAND` does, and Herdr's
//! bridge, probe, installer, and Teleport's scripts all append their remote
//! command to it. A sandbox offers execd's PTY sessions instead: a session
//! runs a command, and its WebSocket in pipe mode carries stdin as `0x00`
//! frames, stdout as `0x01`, stderr as `0x02`, and the exit code in a JSON
//! `exit` message. The app runs itself with [`HELPER`] to translate.
//!
//! execd has no way to end a command's input, which Teleport's scripts need:
//! Git bundles arrive on stdin. So the session runs [`wrapper`], which reads
//! the input as frames, each its length on a line and then its bytes, into a
//! FIFO the real command reads; an empty frame is its end, so the command
//! sees end of file as it would over `ssh`. A frame is read with GNU `head
//! -c`, which never reads past it; other `head`s may read ahead and swallow
//! the frames after it, so elsewhere `dd` reads it a byte at a time. A dropped connection leaves an
//! execd session running for a reconnect that never comes, so the relay
//! sends a heartbeat line while it is quiet, and where the shell can time a
//! read the wrapper ends the input once heartbeats stop: the bridge then
//! exits, as it does when `ssh` drops.

use super::{
    Error, Result,
    api::{Api, Execd},
    http::{self, Auth},
    store,
};
use crate::cloud::Value;
use crate::cloud::socket::{self, Header, pending};
use herdr_client::shell_quote;
use std::{
    ffi::OsString,
    io::{self, Write},
    process::ExitCode,
    sync::mpsc::{Receiver, TryRecvError},
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket};

/// The argument that makes this executable a sandbox relay.
pub const HELPER: &str = "__opensandbox-exec";
/// What `ssh` exits with when the connection, not the command, failed.
const CONNECTION_FAILED: u8 = 255;
/// Input chunks sent before the relay reads again.
const CHUNKS_PER_PASS: usize = 4;
/// How often a quiet relay says it is still there.
const HEARTBEAT: Duration = Duration::from_secs(30);

const STDIN: u8 = 0x00;
const STDOUT: u8 = 0x01;
const STDERR: u8 = 0x02;

/// The shell line execd runs for `remote`: `remote` with framed input, as the
/// module docs describe. The heartbeat timeout is four missed heartbeats.
pub(super) fn wrapper(remote: &str) -> String {
    format!(
        r#"T=; [ -n "${{BASH_VERSION:-}}" ] && T="-t 120"
f=$(mktemp -u /tmp/herdr-relay-XXXXXX) && mkfifo "$f" || exit {CONNECTION_FAILED}
exec 3<&0
if head --version 2>/dev/null | grep -q GNU; then take() {{ head -c "$1"; }}; else take() {{ dd bs=1 count="$1" 2>/dev/null; }}; fi
{{ while IFS= read -r $T n; do case $n in -) ;; 0|''|*[!0-9]*) break ;; *) take "$n" ;; esac; done; }} <&3 >"$f" & r=$!
/bin/sh -c {remote} <"$f"; c=$?
kill $r 2>/dev/null; rm -f "$f"; exit $c"#,
        remote = shell_quote(remote),
    )
}

/// One input frame for the wrapper: `chunk`'s length on a line, then it.
pub(super) fn frame(chunk: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(chunk.len() + 12);
    frame.push(STDIN);
    frame.extend_from_slice(chunk.len().to_string().as_bytes());
    frame.push(b'\n');
    frame.extend_from_slice(chunk);
    frame
}

fn line(text: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(text.len() + 2);
    frame.push(STDIN);
    frame.extend_from_slice(text);
    frame.push(b'\n');
    frame
}

/// Runs this executable as a relay when `args` asks for it, before anything
/// of the app starts.
pub fn run_helper(args: &[OsString]) -> Option<ExitCode> {
    let (first, rest) = args.split_first()?;
    if first != HELPER {
        return None;
    }
    let [account, sandbox, remote @ ..] = rest else {
        eprintln!("Usage: herdr-gpui {HELPER} ACCOUNT SANDBOX COMMAND...");
        return Some(ExitCode::from(2));
    };
    if remote.is_empty() {
        eprintln!("Usage: herdr-gpui {HELPER} ACCOUNT SANDBOX COMMAND...");
        return Some(ExitCode::from(2));
    }
    Some(match run(account, sandbox, remote) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("herdr-gpui: {error}");
            ExitCode::from(CONNECTION_FAILED)
        }
    })
}

fn run(account: &OsString, sandbox: &OsString, remote: &[OsString]) -> Result<u8> {
    let account = account.to_str().ok_or(Error::Encoding(Value::Account))?;
    let sandbox = sandbox.to_str().ok_or(Error::Encoding(Value::SandboxId))?;
    // Like `ssh`, the remote command is its arguments joined by spaces.
    let command = remote
        .iter()
        .map(|part| part.to_str().ok_or(Error::Encoding(Value::RemoteCommand)))
        .collect::<Result<Vec<_>>>()?
        .join(" ");
    let settings = super::connect::settings(account)?;
    let key = store::key(&settings)?;
    let execd = Api {
        settings: &settings,
        key: &key,
    }
    .execd(sandbox)?;
    let session = start(&execd, &command)?;
    let result = attach(&execd, &session).and_then(|mut socket| {
        relay(
            &mut socket,
            socket::input(io::stdin()),
            &mut io::stdout().lock(),
            &mut io::stderr().lock(),
        )
    });
    // The session would otherwise stay for a reconnect that never comes.
    let _ = http::delete(
        "end the exec session",
        &format!("{}/pty/{session}", execd.root),
        &Auth::Endpoint(&execd.headers),
    );
    result
}

/// Start a session running `command` through [`wrapper`]; returns its ID.
pub(super) fn start(execd: &Execd, command: &str) -> Result<String> {
    #[derive(serde::Serialize)]
    struct Create {
        command: String,
    }
    #[derive(serde::Deserialize)]
    struct Created {
        session_id: String,
    }
    let created: Created = http::post(
        "start a command",
        &format!("{}/pty", execd.root),
        &Auth::Endpoint(&execd.headers),
        &Create {
            command: wrapper(command),
        },
    )?;
    if !super::settings::sandbox_id(&created.session_id) {
        return Err(Error::Field(Value::ExecSessionId));
    }
    Ok(created.session_id)
}

pub(super) fn attach(execd: &Execd, session: &str) -> Result<WebSocket<socket::Stream>> {
    let mut url = url::Url::parse(&format!("{}/pty/{session}/ws?pty=0", execd.root))
        .map_err(|_| Error::Field(Value::ExecdEndpoint))?;
    let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
    url.set_scheme(scheme)
        .map_err(|()| Error::Field(Value::ExecdEndpoint))?;
    let headers: Vec<Header> = execd
        .headers
        .iter()
        .map(|(name, value)| Header {
            name,
            value: value.as_bytes(),
            sensitive: true,
        })
        .collect();
    Ok(socket::open(&url, &headers)?)
}

/// What a JSON text frame from execd means for the relay.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Notice {
    Exit(u8),
    Failed(String),
    Other,
}

pub(super) fn notice(text: &str) -> Notice {
    #[derive(serde::Deserialize)]
    struct Frame {
        #[serde(rename = "type")]
        kind: String,
        exit_code: Option<i64>,
        code: Option<String>,
        error: Option<String>,
    }
    let Ok(frame) = serde_json::from_str::<Frame>(text) else {
        return Notice::Other;
    };
    match frame.kind.as_str() {
        "exit" => Notice::Exit(
            frame
                .exit_code
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(CONNECTION_FAILED),
        ),
        "error" => Notice::Failed(
            frame
                .error
                .or(frame.code)
                .unwrap_or_default()
                .chars()
                .filter(|c| !c.is_control())
                .take(200)
                .collect(),
        ),
        _ => Notice::Other,
    }
}

/// Pass `input` to the command and its output to `stdout` and `stderr` until
/// it exits, and return its exit code. The socket's reads must time out.
pub(super) fn relay<S: io::Read + Write>(
    socket: &mut WebSocket<S>,
    input: Receiver<Option<Vec<u8>>>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<u8> {
    let mut input = Some(input);
    let mut sent = Instant::now();
    loop {
        let mut frames = Vec::new();
        for _ in 0..CHUNKS_PER_PASS {
            let Some(receiver) = &input else {
                break;
            };
            match receiver.try_recv() {
                Ok(Some(chunk)) if !chunk.is_empty() => frames.push(frame(&chunk)),
                Ok(Some(_)) => {}
                Ok(None) => {
                    frames.push(line(b"0"));
                    input = None;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    input = None;
                    break;
                }
            }
        }
        if frames.is_empty() && input.is_some() && sent.elapsed() >= HEARTBEAT {
            frames.push(line(b"-"));
        }
        for frame in frames {
            sent = Instant::now();
            // A write that times out has queued its frame; flushing goes on
            // below, between reads.
            match socket.write(Message::Binary(frame.into())) {
                Ok(()) => {}
                Err(tungstenite::Error::Io(error)) if pending(&error) => {}
                Err(error) => return Err(socket::Error::Stream(error).into()),
            }
        }
        match socket.flush() {
            Ok(()) => {}
            Err(tungstenite::Error::Io(error)) if pending(&error) => {}
            Err(error) => return Err(socket::Error::Stream(error).into()),
        }
        match socket.read() {
            Ok(Message::Binary(frame)) => match frame.split_first() {
                Some((&STDOUT, data)) => {
                    stdout.write_all(data).map_err(Error::Output)?;
                    stdout.flush().map_err(Error::Output)?;
                }
                Some((&STDERR, data)) => {
                    stderr.write_all(data).map_err(Error::Output)?;
                    stderr.flush().map_err(Error::Output)?;
                }
                _ => {}
            },
            Ok(Message::Text(text)) => match notice(text.as_str()) {
                Notice::Exit(code) => return Ok(code),
                Notice::Failed(reason) => return Err(Error::Shell(reason)),
                Notice::Other => {}
            },
            Ok(Message::Close(_)) => return Err(socket::Error::Closed.into()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(error)) if pending(&error) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Err(socket::Error::Closed.into());
            }
            Err(error) => return Err(socket::Error::Stream(error).into()),
        }
    }
}
