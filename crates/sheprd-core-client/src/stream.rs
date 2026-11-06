//! Stream channels: an attached terminal's bytes and a subscription's event
//! lines. The connection worker parks what arrives in a route's inbox and
//! never waits for its reader, so a stalled view cannot block channel 0 or
//! another stream; credit flows back only as the reader takes bytes.

use crate::{
    Error, Result, Violation,
    event::{self, Lines, Notice},
    frame::{self, Kind, MAX_PAYLOAD},
    wire::{Display, Terminal},
};
use crossbeam_channel::{Receiver, Sender, TrySendError};
use serde::Deserialize;
use std::{
    sync::{Mutex, MutexGuard, PoisonError},
    time::Duration,
};

/// Bytes a stream may hold before its reader takes them. The core grants
/// 256 KiB of credit and sends a fresh paint past 4 MiB of lag, so this is
/// only reached by a core that ignores credit.
pub const MAX_BUFFERED: usize = 8 << 20;

/// Why a stream ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum End {
    /// The core closed the channel: the program ended, or it answered a detach.
    Closed(Closed),
    /// The connection to the core dropped; the program most likely still runs.
    Lost,
}

/// The core's reason for a CLOSE; every field is empty for a detach answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Closed {
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// `exited`, `killed`, `server_stop`, ...
    #[serde(default)]
    pub reason: Option<String>,
}

impl Closed {
    fn parse(payload: &[u8]) -> Self {
        if payload.is_empty() {
            return Self::default();
        }
        serde_json::from_slice(payload).unwrap_or_default()
    }
}

/// What a stream delivered since the last read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chunk {
    pub bytes: Vec<u8>,
    /// Set once; nothing follows it.
    pub end: Option<End>,
}

#[derive(Default)]
struct Inbox {
    bytes: Vec<u8>,
    end: Option<End>,
    taken_end: bool,
}

/// One channel's inbox and the wake signal its reader waits on.
pub(crate) struct Route {
    inbox: Mutex<Inbox>,
    wake: (Sender<()>, Receiver<()>),
}

impl Route {
    pub(crate) fn new() -> Self {
        Self {
            inbox: Mutex::default(),
            wake: crossbeam_channel::bounded(1),
        }
    }

    fn inbox(&self) -> MutexGuard<'_, Inbox> {
        self.inbox.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn signal(&self) {
        // A full signal already wakes the reader.
        let _ = self.wake.0.try_send(());
    }

    pub(crate) fn push(&self, bytes: &[u8]) -> Result<(), Violation> {
        let mut inbox = self.inbox();
        if inbox.end.is_some() {
            return Ok(());
        }
        if inbox.bytes.len() + bytes.len() > MAX_BUFFERED {
            return Err(Violation::PastCredit);
        }
        inbox.bytes.extend_from_slice(bytes);
        drop(inbox);
        self.signal();
        Ok(())
    }

    pub(crate) fn end(&self, end: End) {
        self.inbox().end.get_or_insert(end);
        self.signal();
    }

    /// Everything waiting, after at most `timeout` for something to arrive.
    fn take(&self, timeout: Duration) -> Chunk {
        let ready =
            |inbox: &Inbox| !inbox.bytes.is_empty() || (inbox.end.is_some() && !inbox.taken_end);
        if !ready(&self.inbox()) {
            let _ = self.wake.1.recv_timeout(timeout);
        }
        let mut inbox = self.inbox();
        let bytes = std::mem::take(&mut inbox.bytes);
        let end = if inbox.taken_end {
            None
        } else {
            inbox.end.clone()
        };
        inbox.taken_end |= end.is_some();
        Chunk { bytes, end }
    }
}

/// The writing half every stream shares with its connection.
#[derive(Clone)]
pub(crate) struct Link {
    pub(crate) outbound: Sender<Vec<u8>>,
}

impl Link {
    pub(crate) fn send(&self, channel: u32, kind: Kind, payload: &[u8]) -> Result<()> {
        let frame = frame::encode(channel, kind, payload)?;
        self.outbound.try_send(frame).map_err(|error| match error {
            TrySendError::Full(_) => Error::Busy,
            TrySendError::Disconnected(_) => Error::Lost,
        })
    }
}

/// A stream channel: reads take what arrived and give its credit back.
pub(crate) struct Stream {
    pub(crate) channel: u32,
    pub(crate) link: Link,
    pub(crate) route: std::sync::Arc<Route>,
}

impl Stream {
    fn read(&self, timeout: Duration) -> Result<Chunk> {
        let chunk = self.route.take(timeout);
        if let Ok(credit) = u32::try_from(chunk.bytes.len())
            && credit > 0
            && chunk.end.is_none()
        {
            self.link
                .send(self.channel, Kind::Credit, &credit.to_le_bytes())?;
        }
        Ok(chunk)
    }

    fn close(&self) {
        // A lost connection has nothing left to close.
        let _ = self.link.send(self.channel, Kind::Close, &[]);
    }
}

/// One attached terminal: its first paint and live output, and the viewer's
/// keys, size, focus, and display facts going back.
pub struct Attachment {
    stream: Stream,
    /// The terminal as it was when attached.
    pub terminal: Terminal,
}

impl Attachment {
    pub(crate) fn new(stream: Stream, terminal: Terminal) -> Self {
        Self { stream, terminal }
    }

    pub fn channel(&self) -> u32 {
        self.stream.channel
    }

    /// What arrived, waiting at most `timeout`. Blocking: worker threads only.
    pub fn read(&self, timeout: Duration) -> Result<Chunk> {
        self.stream.read(timeout)
    }

    /// Bytes for the program: keys already encoded for its modes.
    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        bytes.chunks(MAX_PAYLOAD).try_for_each(|chunk| {
            self.stream
                .link
                .send(self.stream.channel, Kind::Data, chunk)
        })
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let mut payload = [0; 4];
        payload[..2].copy_from_slice(&cols.to_le_bytes());
        payload[2..].copy_from_slice(&rows.to_le_bytes());
        self.stream
            .link
            .send(self.stream.channel, Kind::Resize, &payload)
    }

    pub fn focus(&self, focused: bool) -> Result<()> {
        self.stream
            .link
            .send(self.stream.channel, Kind::Focus, &[u8::from(focused)])
    }

    pub fn facts(&self, display: &Display) -> Result<()> {
        let payload = serde_json::to_vec(display)?;
        self.stream
            .link
            .send(self.stream.channel, Kind::Facts, &payload)
    }
}

impl Drop for Attachment {
    fn drop(&mut self) {
        self.stream.close();
    }
}

/// Terminal events after a snapshot.
pub struct Subscription {
    ended: bool,
    lines: Lines,
    stream: Stream,
    /// Events after this sequence follow.
    pub seq: u64,
    /// Every matching terminal when the subscription started.
    pub terminals: Vec<Terminal>,
}

impl Subscription {
    pub(crate) fn new(stream: Stream, seq: u64, terminals: Vec<Terminal>) -> Self {
        Self {
            ended: false,
            lines: Lines::default(),
            stream,
            seq,
            terminals,
        }
    }

    /// The notices that arrived, waiting at most `timeout`. The stream
    /// ending is `Error::Lost`. Blocking: worker threads only.
    pub fn next(&mut self, timeout: Duration) -> Result<Vec<Notice>> {
        if self.ended {
            return Err(Error::Lost);
        }
        let chunk = self.stream.read(timeout)?;
        let mut notices = Vec::new();
        for line in self.lines.push(&chunk.bytes)? {
            match event::parse(&line) {
                Ok(Some(notice)) => notices.push(notice),
                Ok(None) => {}
                // One unreadable event must not end every later one.
                Err(error) => tracing::warn!(%error, "Skipped an unreadable herdr-core event"),
            }
        }
        self.ended = chunk.end.is_some();
        if self.ended && notices.is_empty() {
            return Err(Error::Lost);
        }
        Ok(notices)
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.stream.close();
    }
}

pub(crate) fn closed(payload: &[u8]) -> End {
    End::Closed(Closed::parse(payload))
}
