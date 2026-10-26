//! A read-only view of one explicitly selected CLI transcript. No agent is
//! spawned, no Herdr reports are sent, and no transcript command is executed.

mod error;
pub(crate) mod transcript;

pub use error::{Error, Result};

use std::{
    collections::HashMap,
    io::{self, Write},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use crossbeam_channel::{Sender, bounded};
use herdr_chat_protocol::{
    Change, Cursor, EndpointId, Rejection, ServerMessage, Transcript, VERSION,
};

use crate::{
    cli::MirrorOptions,
    session::{ConnId, Input},
};

/// How often a watched transcript is checked for changes.
pub(crate) const POLL: Duration = Duration::from_secs(1);
const MAX_CLIENTS: usize = 8;

pub(crate) fn run(options: MirrorOptions) -> anyhow::Result<()> {
    let mut source = transcript::Source::new(options.path, options.agent);
    let initial = source.read()?.context("the transcript could not be read")?;
    let mut mirror = Mirror::new(initial, options.pane);
    let id = options.endpoint.unwrap_or_else(EndpointId::random);
    let address = herdr_chat_protocol::endpoint_address(&id)
        .context("neither XDG_STATE_HOME nor HOME is set")?;
    let (inputs, receiver) = bounded(128);
    let _server = crate::attach::Server::listen(&address, inputs)?;
    // Only connection metadata goes to stdout, never transcript content.
    println!("{id}");
    io::stdout().flush()?;
    let mut next_poll = Instant::now() + POLL;
    loop {
        match receiver.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
            Ok(Input::Shutdown) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Ok(input) => mirror.input(input),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }
        if Instant::now() >= next_poll {
            // A CLI that replaces its log by renaming a new file over it
            // leaves the path briefly missing. The view keeps what it last
            // showed and the next poll reads whatever file holds the path,
            // which `update` turns into a fresh snapshot when it was rewritten.
            if let Ok(Some(next)) = source.read() {
                mirror.update(next);
            }
            next_poll = Instant::now() + POLL;
        }
    }
    Ok(())
}

pub(crate) struct Mirror {
    clients: HashMap<ConnId, Sender<ServerMessage>>,
    cursor: Cursor,
    pane: Option<String>,
    transcript: Transcript,
}

impl Mirror {
    pub(crate) fn new(transcript: Transcript, pane: Option<String>) -> Self {
        Self {
            clients: HashMap::new(),
            cursor: Cursor::new_epoch(),
            pane,
            transcript,
        }
    }

    fn snapshot(&self) -> ServerMessage {
        ServerMessage::Snapshot {
            cursor: self.cursor,
            transcript: Box::new(self.transcript.clone()),
        }
    }

    pub(crate) fn input(&mut self, input: Input) {
        match input {
            Input::Attach(attach) => {
                if self.clients.len() >= MAX_CLIENTS {
                    return;
                }
                let welcome = ServerMessage::Welcome {
                    agent: self.transcript.agent.clone(),
                    pane: self.pane.clone(),
                    version: VERSION,
                };
                if attach.outbox.try_send(welcome).is_ok()
                    && attach.outbox.try_send(self.snapshot()).is_ok()
                {
                    self.clients.insert(attach.conn, attach.outbox);
                }
            }
            Input::Detach(conn) => {
                self.clients.remove(&conn);
            }
            Input::Command { conn, id, .. } | Input::Unsupported { conn, id } => {
                // This applies to every command, including Cancel and Answer:
                // old log records never become actionable permission requests.
                if let Some(client) = self.clients.get(&conn)
                    && client
                        .try_send(ServerMessage::Ack {
                            id,
                            rejected: Some(Rejection::NotReady),
                        })
                        .is_err()
                {
                    self.clients.remove(&conn);
                }
            }
            Input::Agent(_) | Input::AgentExited { .. } | Input::Shutdown => {}
        }
    }

    fn broadcast(&mut self, message: &ServerMessage) {
        self.clients
            .retain(|_, client| client.try_send(message.clone()).is_ok());
    }

    pub(crate) fn update(&mut self, next: Transcript) {
        if next == self.transcript {
            return;
        }
        let Some(changes) = changes(&self.transcript, &next) else {
            // Rewrites/rotation can remove or reorder history. A new epoch
            // makes every client replace it instead of merging two histories.
            self.cursor = Cursor::new_epoch();
            self.transcript = next;
            self.broadcast(&self.snapshot());
            return;
        };
        self.transcript = next;
        for change in changes {
            self.cursor.seq += 1;
            self.broadcast(&ServerMessage::Change {
                change,
                cursor: self.cursor,
            });
        }
    }
}

/// The existing reducer keeps scroll position and prepared bodies stable on
/// ordinary appends; only a rewritten file needs another snapshot.
fn changes(previous: &Transcript, next: &Transcript) -> Option<Vec<Change>> {
    let mut changes = Vec::new();
    let first = next.entries.first()?.id;
    let retained: Vec<_> = previous
        .entries
        .iter()
        .filter(|entry| entry.id >= first)
        .collect();
    if retained.len() > next.entries.len()
        || retained
            .iter()
            .zip(&next.entries)
            .any(|(old, new)| old.id != new.id)
        || (previous.truncated && !next.truncated)
    {
        return None;
    }
    if retained.len() < previous.entries.len() {
        changes.push(Change::Truncate { before: first });
    }
    for (old, new) in retained.iter().zip(&next.entries) {
        if **old != *new {
            changes.push(Change::Replace { entry: new.clone() });
        }
    }
    for entry in next.entries.iter().skip(retained.len()) {
        changes.push(Change::Push {
            entry: entry.clone(),
        });
    }
    if previous.session != next.session {
        changes.push(Change::Session {
            session: next.session.clone(),
        });
    }
    if previous.options != next.options {
        changes.push(Change::Options {
            options: next.options.clone(),
        });
    }
    if previous.usage != next.usage {
        changes.push(Change::Usage {
            usage: next.usage.clone(),
        });
    }
    // If truncation was caused by skipping a record, rather than dropping an
    // entry, only a snapshot can carry the flag accurately.
    if next.truncated
        && !previous.truncated
        && !changes
            .iter()
            .any(|change| matches!(change, Change::Truncate { .. }))
    {
        return None;
    }
    Some(changes)
}

#[cfg(test)]
mod tests;
