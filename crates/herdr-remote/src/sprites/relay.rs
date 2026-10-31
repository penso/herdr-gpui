//! This executable as a sprite's standard streams. Cloud devices reach their
//! machines by running a command whose stdin and stdout are the remote shell's,
//! the way `ssh host COMMAND` does, and Herdr's bridge, probe, installer, and
//! Teleport's scripts all append their remote command to it. A sprite offers
//! no SSH server, only the API's exec stream: a WebSocket whose binary frames
//! carry one stream byte and then data. The app runs itself with [`HELPER`]
//! to translate between the two.
//!
//! Frames from the relay: `0x00` and stdin bytes, and `0x04` once stdin ends.
//! Frames to it: `0x01` stdout, `0x02` stderr, and `0x03` with the exit code;
//! JSON text frames carry session and port notices, and an `exit` message.
//! Without a terminal the bytes pass unchanged, so the bridge's binary
//! protocol survives the trip.

use super::{Error, Result, http::authorization, store};
use crate::cloud::Value;
use crate::cloud::socket::{self, Header, pending};
use std::{
    ffi::OsString,
    io::{self, Read, Write},
    process::ExitCode,
    sync::mpsc::{Receiver, TryRecvError},
};
use tungstenite::{Message, WebSocket};
use url::Url;

/// The argument that makes this executable a sprite relay.
pub const HELPER: &str = "__sprites-exec";
/// What `ssh` exits with when the connection, not the command, failed. The
/// bridge and installer read it the same way here.
const CONNECTION_FAILED: u8 = 255;
/// Input chunks sent before the relay reads again, so steady input, such as
/// a Git bundle, never keeps the command's output waiting.
const CHUNKS_PER_PASS: usize = 4;

const STDIN: u8 = 0x00;
const STDOUT: u8 = 0x01;
const STDERR: u8 = 0x02;
const EXIT: u8 = 0x03;
const STDIN_EOF: u8 = 0x04;

/// Runs this executable as a relay when `args` asks for it, before anything
/// of the app starts.
pub fn run_helper(args: &[OsString]) -> Option<ExitCode> {
    let (first, rest) = args.split_first()?;
    if first != HELPER {
        return None;
    }
    let [account, sprite, remote @ ..] = rest else {
        eprintln!("Usage: herdr-gpui {HELPER} ACCOUNT SPRITE COMMAND...");
        return Some(ExitCode::from(2));
    };
    if remote.is_empty() {
        eprintln!("Usage: herdr-gpui {HELPER} ACCOUNT SPRITE COMMAND...");
        return Some(ExitCode::from(2));
    }
    Some(match run(account, sprite, remote) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("herdr-gpui: {error}");
            ExitCode::from(CONNECTION_FAILED)
        }
    })
}

fn run(account: &OsString, sprite: &OsString, remote: &[OsString]) -> Result<u8> {
    let account = account.to_str().ok_or(Error::Encoding(Value::Account))?;
    let sprite = sprite.to_str().ok_or(Error::Encoding(Value::SpriteName))?;
    // Like `ssh`, the remote command is its arguments joined by spaces.
    let command = remote
        .iter()
        .map(|part| part.to_str().ok_or(Error::Encoding(Value::RemoteCommand)))
        .collect::<Result<Vec<_>>>()?
        .join(" ");
    let settings = super::connect::settings(account)?;
    let url = settings.exec_url(sprite, &command)?;
    let mut socket = open(&url, &store::token(&settings)?)?;
    let code = relay(
        &mut socket,
        socket::input(io::stdin()),
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )?;
    let _ = socket.close(None);
    Ok(code)
}

/// Open the exec stream at `url` with `token`.
pub(super) fn open(url: &Url, token: &secrecy::SecretString) -> Result<WebSocket<socket::Stream>> {
    let authorization = authorization(token)?;
    Ok(socket::open(
        url,
        &[Header {
            name: "Authorization",
            value: authorization.as_bytes(),
            sensitive: true,
        }],
    )?)
}

/// The exit code in an `exit` text message, if `text` is one.
fn exit_message(text: &str) -> Option<u8> {
    #[derive(serde::Deserialize)]
    struct Exit {
        #[serde(rename = "type")]
        kind: String,
        exit_code: i64,
    }
    let exit = serde_json::from_str::<Exit>(text).ok()?;
    (exit.kind == "exit").then(|| u8::try_from(exit.exit_code).unwrap_or(CONNECTION_FAILED))
}

/// Pass `input` to the command and its output to `stdout` and `stderr` until
/// it exits, and return its exit code. The socket's reads must time out, so
/// input keeps flowing while the command is quiet.
pub(super) fn relay<S: Read + Write>(
    socket: &mut WebSocket<S>,
    input: Receiver<Option<Vec<u8>>>,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<u8> {
    let mut input = Some(input);
    loop {
        for _ in 0..CHUNKS_PER_PASS {
            let Some(receiver) = &input else {
                break;
            };
            let frame = match receiver.try_recv() {
                Ok(Some(chunk)) => {
                    let mut frame = Vec::with_capacity(chunk.len() + 1);
                    frame.push(STDIN);
                    frame.extend_from_slice(&chunk);
                    frame
                }
                Ok(None) => {
                    input = None;
                    vec![STDIN_EOF]
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    input = None;
                    break;
                }
            };
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
                Some((&EXIT, code)) => return Ok(code.first().copied().unwrap_or(0)),
                _ => {}
            },
            Ok(Message::Text(text)) => {
                if let Some(code) = exit_message(text.as_str()) {
                    return Ok(code);
                }
            }
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
