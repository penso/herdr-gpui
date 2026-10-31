//! Event notifications: JSON lines on a subscription's channel.

use crate::{
    Error, Result, Violation,
    wire::{Foreground, Status, Terminal},
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// The longest event line accepted: one frame's payload.
pub const MAX_LINE: usize = crate::frame::MAX_PAYLOAD;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    Event(Event),
    Heartbeat {
        seq: u64,
    },
    /// Events were dropped: every matching terminal now. Ignore events whose
    /// `seq` is not above this one.
    Missed {
        seq: u64,
        terminals: Vec<Terminal>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub change: TerminalChange,
    pub seq: u64,
    pub terminal: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalChange {
    // keep-sorted start
    Created(Box<Terminal>),
    Finished(Ended),
    Foreground(Option<Foreground>),
    Keep(bool),
    Labels {
        labels: BTreeMap<String, String>,
        record: Option<Box<Terminal>>,
    },
    Lost(Ended),
    Removed,
    Restarted(Box<Terminal>),
    Size {
        cols: u16,
        rows: u16,
    },
    Status {
        reported: bool,
        status: Option<Status>,
    },
    Title(Option<String>),
    Turn {
        state: Option<String>,
        turn: u64,
    },
    /// An event type newer than this client.
    Unknown(String),
    // keep-sorted end
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Ended {
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub signal: Option<Value>,
}

#[derive(Deserialize)]
struct Message {
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Deserialize)]
struct RawEvent {
    #[serde(default)]
    data: Value,
    seq: u64,
    terminal: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct Seq {
    seq: u64,
}

#[derive(Deserialize)]
struct MissedParams {
    seq: u64,
    #[serde(default)]
    terminals: Vec<Terminal>,
}

#[derive(Deserialize)]
struct StatusData {
    #[serde(default)]
    reported: bool,
    #[serde(default)]
    status: Option<Status>,
}

#[derive(Deserialize)]
struct TurnData {
    #[serde(default)]
    state: Option<String>,
    turn: u64,
}

#[derive(Deserialize)]
struct LabelsData {
    #[serde(default)]
    labels: BTreeMap<String, String>,
    #[serde(default)]
    record: Option<Box<Terminal>>,
}

#[derive(Deserialize)]
struct SizeData {
    cols: u16,
    rows: u16,
}

#[derive(Deserialize)]
struct KeepData {
    keep: bool,
}

#[derive(Deserialize)]
struct TitleData {
    #[serde(default)]
    title: Option<String>,
}

/// One notification, or none for a method this client does not know.
pub fn parse(line: &[u8]) -> Result<Option<Notice>> {
    let message: Message = serde_json::from_slice(line)?;
    Ok(Some(match message.method.as_str() {
        "event" => Notice::Event(event(serde_json::from_value(message.params)?)?),
        "heartbeat" => {
            let Seq { seq } = serde_json::from_value(message.params)?;
            Notice::Heartbeat { seq }
        }
        "missed" => {
            let MissedParams { seq, terminals } = serde_json::from_value(message.params)?;
            Notice::Missed { seq, terminals }
        }
        _ => return Ok(None),
    }))
}

fn event(raw: RawEvent) -> Result<Event> {
    let data = raw.data;
    let change = match raw.kind.as_str() {
        "created" => TerminalChange::Created(serde_json::from_value(data)?),
        "finished" => TerminalChange::Finished(serde_json::from_value(data)?),
        "foreground" => TerminalChange::Foreground(serde_json::from_value(data)?),
        "keep" => TerminalChange::Keep(serde_json::from_value::<KeepData>(data)?.keep),
        "labels" => {
            let LabelsData { labels, record } = serde_json::from_value(data)?;
            TerminalChange::Labels { labels, record }
        }
        "lost" => TerminalChange::Lost(serde_json::from_value(data)?),
        "removed" => TerminalChange::Removed,
        "restarted" => TerminalChange::Restarted(serde_json::from_value(data)?),
        "size" => {
            let SizeData { cols, rows } = serde_json::from_value(data)?;
            TerminalChange::Size { cols, rows }
        }
        "status" => {
            let StatusData { reported, status } = serde_json::from_value(data)?;
            TerminalChange::Status { reported, status }
        }
        "title" => TerminalChange::Title(serde_json::from_value::<TitleData>(data)?.title),
        "turn" => {
            let TurnData { state, turn } = serde_json::from_value(data)?;
            TerminalChange::Turn { state, turn }
        }
        other => TerminalChange::Unknown(other.to_owned()),
    };
    Ok(Event {
        change,
        seq: raw.seq,
        terminal: raw.terminal,
    })
}

/// Splits a byte stream into lines; frame boundaries can fall anywhere.
#[derive(Debug, Default)]
pub struct Lines {
    partial: Vec<u8>,
}

impl Lines {
    /// Every whole line in `bytes` after what came before, without newlines.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
        let mut lines = Vec::new();
        for piece in bytes.split_inclusive(|byte| *byte == b'\n') {
            self.partial.extend_from_slice(piece);
            if self.partial.len() > MAX_LINE + 1 {
                return Err(Error::Protocol(Violation::LongLine(MAX_LINE)));
            }
            if self.partial.ends_with(b"\n") {
                self.partial.pop();
                let line = std::mem::take(&mut self.partial);
                if !line.iter().all(u8::is_ascii_whitespace) {
                    lines.push(line);
                }
            }
        }
        Ok(lines)
    }
}
