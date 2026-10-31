//! The relay itself: the helper process, its lines, and the frames it
//! carries.

use super::Error;
use super::Radio;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use herdr_remote_control::control::{self, Request, Response};
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread,
};

/// The helper's file name, beside the app's executable.
pub(crate) const PROGRAM: &str = if cfg!(windows) {
    "herdr-nearby.exe"
} else {
    "herdr-nearby"
};
/// The largest message a phone or the daemon sends through the helper.
pub(crate) const MAX_MESSAGE: usize = 4096;
/// The longest line read from the helper: a base64 frame of
/// [`MAX_MESSAGE`] bytes and its envelope.
const MAX_LINE: usize = 8 * 1024;
/// Lines waiting in each direction; a full queue drops the newest.
const QUEUE: usize = 16;
/// Tells the helper to stop advertising for pairing, and before its input
/// closes, everything.
pub(crate) const STOP: &str = r#"{"type":"stop"}"#;

/// One line from the helper.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Event {
    Radio(Radio),
    /// A whole message a phone wrote, and which phone to answer.
    Frame {
        central: String,
        data: Vec<u8>,
    },
    /// What the helper could not do, for the log.
    Failure(String),
    /// A whole `presence` message a phone wrote.
    PresenceFrame {
        central: String,
        data: Vec<u8>,
    },
    /// A phone left: out of range, or it closed the connection.
    Disconnected {
        central: String,
    },
    /// Seconds since the desktop's last input.
    Idle(u64),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum Line {
    State { state: String },
    Frame { central: String, data: String },
    PresenceFrame { central: String, data: String },
    Disconnected { central: String },
    Idle { seconds: u64 },
    Error { message: String },
}

/// A phone's message, as `POST /api/nearby/*` reads it.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum PhoneMessage {
    Start { public_key: String },
    Reveal { session: String, nonce: String },
}

/// Reads one helper line; anything malformed or oversized is `None`.
pub(crate) fn parse_line(line: &str) -> Option<Event> {
    match serde_json::from_str::<Line>(line).ok()? {
        Line::State { state } => Some(Event::Radio(match state.as_str() {
            "powered_on" => Radio::On,
            "powered_off" => Radio::Off,
            "unauthorized" => Radio::Unauthorized,
            "unsupported" => Radio::Unsupported,
            _ => Radio::Unknown,
        })),
        Line::Frame { central, data } => {
            let data = STANDARD.decode(data).ok()?;
            (central_id(&central) && data.len() <= MAX_MESSAGE)
                .then_some(Event::Frame { central, data })
        }
        Line::PresenceFrame { central, data } => {
            let data = STANDARD.decode(data).ok()?;
            (central_id(&central) && data.len() <= MAX_MESSAGE)
                .then_some(Event::PresenceFrame { central, data })
        }
        Line::Disconnected { central } => {
            central_id(&central).then_some(Event::Disconnected { central })
        }
        Line::Idle { seconds } => Some(Event::Idle(seconds)),
        Line::Error { message } => Some(Event::Failure(message.chars().take(200).collect())),
    }
}

/// Whether `central` is the UUID Core Bluetooth names a phone by.
fn central_id(central: &str) -> bool {
    central.len() == 36
        && central
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
}

/// The control request a phone's frame asks for.
pub(crate) fn request(frame: &[u8]) -> Result<Request, Error> {
    if frame.len() > MAX_MESSAGE {
        return Err(Error::TooLarge { limit: MAX_MESSAGE });
    }
    Ok(
        match serde_json::from_slice(frame).map_err(Error::Malformed)? {
            PhoneMessage::Start { public_key } => Request::NearbyStart { public_key },
            PhoneMessage::Reveal { session, nonce } => Request::NearbyReveal { session, nonce },
        },
    )
}

/// The JSON the phone gets back: the daemon's answer exactly as the HTTP
/// route writes it, or `{"error": …}`.
pub(crate) fn answer(result: Result<Response, control::Error>) -> Vec<u8> {
    let value = match result {
        Ok(Response::NearbyStarted(started)) => serde_json::to_value(started),
        Ok(Response::NearbyRevealed(revealed)) => serde_json::to_value(revealed),
        Ok(Response::Error { message, .. }) => Ok(json!({ "error": message })),
        Ok(_) => Ok(json!({ "error": "the computer gave an answer this relay does not read" })),
        Err(error) => Ok(json!({ "error": error.to_string() })),
    };
    value
        .unwrap_or_else(|_| json!({ "error": "the answer could not be written" }))
        .to_string()
        .into_bytes()
}

/// An answer for a frame that was not a nearby message.
pub(crate) fn refusal() -> Vec<u8> {
    json!({ "error": "that message is not one Herdr reads" })
        .to_string()
        .into_bytes()
}

/// Starts advertising as `name`, with the first 16 hex digits of the CA
/// fingerprint so a phone tells new computers from paired ones, and the
/// daemon's HTTPS port, which the macOS helper also registers with the
/// system's DNS-SD: the daemon's own Bonjour advertisement is invisible to
/// mDNSResponder on the same Mac. Elsewhere the daemon advertises Bonjour
/// itself, and the helper ignores the port.
pub(crate) fn advertise(name: &str, fingerprint: &str, port: Option<u16>) -> String {
    let prefix: String = fingerprint
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(16)
        .collect::<String>()
        .to_ascii_uppercase();
    let mut message = json!({ "type": "advertise", "name": name, "fp": prefix });
    if let Some(port) = port {
        message["port"] = json!(port.to_string());
    }
    message.to_string()
}

/// The port of the offer's link, `https://ADDRESS:PORT/pair#…`, where the
/// daemon serves HTTPS; brackets keep an IPv6 address's colons apart.
pub(crate) fn port(offer_url: &str) -> Option<u16> {
    let rest = offer_url.strip_prefix("https://")?;
    let authority = rest.split(['/', '#', '?']).next()?;
    let (host, port) = authority.rsplit_once(':')?;
    if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
        return None;
    }
    port.parse().ok()
}

/// Writes `data` back to the phone `central`.
pub(crate) fn frame(central: &str, data: &[u8]) -> String {
    json!({ "type": "frame", "central": central, "data": STANDARD.encode(data) }).to_string()
}

/// Writes `data` back to the phone `central` on `presence`.
pub(crate) fn presence_frame(central: &str, data: &[u8]) -> String {
    json!({ "type": "presence_frame", "central": central, "data": STANDARD.encode(data) })
        .to_string()
}

/// Turns advertising for paired phones' presence on or off; pairing's
/// advertisement goes on beside it.
pub(crate) fn advertise_presence(on: bool) -> String {
    json!({ "type": "advertise_presence", "on": on }).to_string()
}

/// The helper beside `exe`, never one found on `PATH`: it is part of the
/// app, so a development build without it pairs over the network only.
pub(crate) fn helper_path(exe: Option<&Path>) -> Option<PathBuf> {
    crate::bundled::locate_in(PROGRAM, exe, None)
}

/// The name a phone lists before pairing; the daemon's own name arrives
/// with its first answer.
pub(crate) fn host_name() -> String {
    #[cfg(unix)]
    {
        let uname = rustix::system::uname();
        let name = uname.nodename().to_string_lossy();
        let name = name.strip_suffix(".local").unwrap_or(&name).trim();
        if !name.is_empty() {
            return name.chars().take(64).collect();
        }
    }
    std::env::var("COMPUTERNAME")
        .ok()
        .map(|name| name.trim().chars().take(64).collect::<String>())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Herdr".to_owned())
}

/// Reads one line of at most [`MAX_LINE`] bytes into `line`. Returns
/// `Ok(false)` at the end of input; an overlong line is skipped whole and
/// leaves `line` empty.
pub(crate) fn read_line(reader: &mut impl BufRead, line: &mut Vec<u8>) -> io::Result<bool> {
    line.clear();
    let mut overlong = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(!line.is_empty() || overlong);
        }
        let (chunk, done) = match available.iter().position(|&byte| byte == b'\n') {
            Some(end) => (&available[..end], Some(end + 1)),
            None => (available, None),
        };
        if !overlong {
            if line.len() + chunk.len() > MAX_LINE {
                overlong = true;
                line.clear();
            } else {
                line.extend_from_slice(chunk);
            }
        }
        let used = done.unwrap_or(available.len());
        reader.consume(used);
        if done.is_some() {
            return Ok(true);
        }
    }
}

/// The running helper: its lines arrive on `events`, lines to it go
/// through `send`. Dropping it tells the helper to stop, closes its input,
/// and kills and waits for that exact child on its own thread.
pub(crate) struct Helper {
    pub(crate) events: Receiver<Event>,
    writes: SyncSender<String>,
    /// Dropped with the helper, which wakes the reaper.
    _stop: mpsc::Sender<()>,
}

impl Helper {
    /// Starts the helper at `path`. Blocking: run it off the UI thread.
    pub(crate) fn spawn(path: &Path) -> Result<Self, Error> {
        let mut child = herdr_client::new_command(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(Error::Spawn)?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            reap(child);
            return Err(Error::Pipe("standard"));
        };
        let (events_in, events) = mpsc::sync_channel(QUEUE);
        thread::spawn(move || {
            let mut reader = io::BufReader::new(stdout);
            let mut line = Vec::new();
            while let Ok(true) = read_line(&mut reader, &mut line) {
                let Some(event) = std::str::from_utf8(&line).ok().and_then(parse_line) else {
                    continue;
                };
                if let Err(TrySendError::Disconnected(_)) = events_in.try_send(event) {
                    return;
                }
            }
        });
        let (writes, lines) = mpsc::sync_channel::<String>(QUEUE);
        thread::spawn(move || {
            let mut stdin = stdin;
            for line in lines {
                if stdin
                    .write_all(line.as_bytes())
                    .and_then(|()| stdin.write_all(b"\n"))
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    return;
                }
            }
            // Closing its input ends the helper.
        });
        let (stop, stopped) = mpsc::channel::<()>();
        thread::spawn(move || {
            // Returns when the helper is dropped and the sender with it.
            let _ = stopped.recv();
            reap(child);
        });
        Ok(Self {
            events,
            writes,
            _stop: stop,
        })
    }

    /// Queues `line` for the helper; drops it when the queue is full.
    pub(crate) fn send(&self, line: String) {
        let _ = self.writes.try_send(line);
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        self.send(STOP.to_owned());
    }
}

/// Kills and waits for `child`, which this thread owns.
fn reap(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
