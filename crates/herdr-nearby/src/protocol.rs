//! The lines Herdr UI and the helper exchange on stdio: one JSON object
//! per line, the same messages the macOS helper reads and writes (its
//! header comment in `macos/herdr-nearby/main.swift`).

use crate::framing::MAX_MESSAGE;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

/// The longest line accepted from Herdr UI: a base64 frame of
/// [`MAX_MESSAGE`] bytes and its envelope.
pub(crate) const MAX_LINE: usize = 8 * 1024;

/// Which characteristic a phone's message came on, or goes back on.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Channel {
    Pair,
    Presence,
}

/// What Herdr UI asks of the helper.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Command {
    /// Advertise for pairing as `name`, with the CA fingerprint's first 16
    /// hex digits in `info`.
    Advertise { name: String, fingerprint: String },
    /// Advertise for paired phones' presence, or stop.
    Presence(bool),
    /// Stop pairing's advertisement; presence's goes on.
    Stop,
    /// A message for one phone.
    Frame {
        central: String,
        channel: Channel,
        data: Vec<u8>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum Line {
    Advertise {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        fp: Option<String>,
    },
    AdvertisePresence {
        #[serde(default)]
        on: bool,
    },
    Frame {
        central: String,
        data: String,
    },
    PresenceFrame {
        central: String,
        data: String,
    },
    Stop,
}

impl Command {
    /// Reads one line; anything malformed, unknown, or oversized is `None`,
    /// as the macOS helper ignores it.
    pub(crate) fn parse(line: &str) -> Option<Self> {
        if line.len() > MAX_LINE {
            return None;
        }
        let frame = |central: String, channel, data: String| {
            let data = STANDARD.decode(data).ok()?;
            (data.len() <= MAX_MESSAGE).then_some(Self::Frame {
                central,
                channel,
                data,
            })
        };
        match serde_json::from_str::<Line>(line).ok()? {
            Line::Advertise { name, fp } => Some(Self::Advertise {
                name: name.unwrap_or_else(|| "Herdr".to_owned()),
                fingerprint: fp.unwrap_or_default(),
            }),
            Line::AdvertisePresence { on } => Some(Self::Presence(on)),
            Line::Stop => Some(Self::Stop),
            Line::Frame { central, data } => frame(central, Channel::Pair, data),
            Line::PresenceFrame { central, data } => frame(central, Channel::Presence, data),
        }
    }
}

/// The radio, named as Core Bluetooth names its states. BlueZ and WinRT
/// have no "unauthorized" or "resetting" to report.
#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Radio {
    PoweredOn,
    PoweredOff,
    Unsupported,
    Unknown,
}

/// What the helper tells Herdr UI.
#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub(crate) enum Event {
    State { state: Radio },
    Frame { central: String, data: String },
    PresenceFrame { central: String, data: String },
    Disconnected { central: String },
    Idle { seconds: u64 },
    Error { message: String },
}

impl Event {
    /// A whole message a phone wrote, for Herdr UI to relay.
    pub(crate) fn frame(central: &str, channel: Channel, data: &[u8]) -> Self {
        let central = central.to_owned();
        let data = STANDARD.encode(data);
        match channel {
            Channel::Pair => Self::Frame { central, data },
            Channel::Presence => Self::PresenceFrame { central, data },
        }
    }

    pub(crate) fn error(message: impl std::fmt::Display) -> Self {
        Self::Error {
            message: message.to_string().chars().take(200).collect(),
        }
    }
}

/// Writes events to Herdr UI, one line each; shared by every thread that
/// reports something.
#[derive(Clone, Debug, Default)]
pub(crate) struct Out;

impl Out {
    pub(crate) fn emit(&self, event: &Event) {
        let Ok(mut line) = serde_json::to_vec(event) else {
            return;
        };
        line.push(b'\n');
        // One locked write per line keeps lines whole across threads; a
        // closed pipe means Herdr UI went away, and stdin's end follows.
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(&line).and_then(|()| stdout.flush());
    }
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
        let (taken, done) = match available.iter().position(|byte| *byte == b'\n') {
            Some(end) => (end + 1, true),
            None => (available.len(), false),
        };
        if !overlong {
            line.extend_from_slice(&available[..taken]);
            if line.len() > MAX_LINE + 1 {
                overlong = true;
                line.clear();
            }
        }
        reader.consume(taken);
        if done {
            if overlong {
                line.clear();
            }
            return Ok(true);
        }
    }
}
