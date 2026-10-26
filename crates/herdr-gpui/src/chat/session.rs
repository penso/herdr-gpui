//! What the window knows of one chat: the transcript as of its cursor, and
//! the commands the program has not acknowledged yet. Pure state, applied
//! from a link's messages, so ordering and fencing are tested without a
//! socket.

use super::{Error, MAX_PENDING, Result};
use herdr_chat_protocol::{
    Change, ClientMessage, Command, CommandId, Cursor, EntryId, Rejection, ServerMessage,
    Transcript, VERSION,
};
use std::collections::HashMap;

/// Connections a command reaches before it is given up: a program that
/// drops the connection on a command it cannot read would otherwise get it
/// again on every reconnect and never come back. Only connections the
/// program greeted count, so an outage alone never drops a prompt.
const SENDS: u8 = 2;

/// A command sent and not yet acknowledged, resent on a new connection.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    command: Command,
    id: CommandId,
    /// Greeted connections it went out on.
    sends: u8,
}

#[derive(Debug, Default)]
pub(crate) struct Session {
    /// The agent the program greeted with, until a transcript names one.
    agent: String,
    /// The last change applied, or `None` when the next connection must
    /// start from a snapshot.
    cursor: Option<Cursor>,
    pending: Vec<Pending>,
    /// The last command the program refused, shown until the next one.
    rejection: Option<Rejection>,
    /// The revision each entry last changed at, so the view redraws only
    /// the entries that did.
    revisions: HashMap<EntryId, u64>,
    revision: u64,
    /// Snapshots applied, so the view starts its list over after one.
    snapshots: u64,
    transcript: Transcript,
    /// Whether the current connection has greeted.
    welcomed: bool,
}

impl Session {
    /// A new connection: it must greet before anything else counts.
    pub(crate) fn begin(&mut self) {
        self.welcomed = false;
    }

    /// The connection ended. After a change that did not fit, the next one
    /// asks for a snapshot; the transcript stays on screen meanwhile.
    pub(crate) fn lost(&mut self, error: &Error) {
        self.welcomed = false;
        if error.desynced() {
            self.cursor = None;
        }
    }

    /// Where the next connection resumes.
    pub(crate) fn since(&self) -> Option<Cursor> {
        self.cursor
    }

    /// Whether a transcript has arrived since the session began.
    pub(crate) fn synced(&self) -> bool {
        self.snapshots > 0
    }

    /// Greeted, and holding the program's current transcript.
    pub(crate) fn live(&self) -> bool {
        self.welcomed && self.cursor.is_some()
    }

    pub(crate) fn transcript(&self) -> &Transcript {
        &self.transcript
    }

    /// A log observer cannot infer live execution state from historical text.
    /// Its daemon supplies status without changing the transcript wire cursor.
    pub(crate) fn set_observed_status(&mut self, status: herdr_chat_protocol::Status) -> bool {
        if !self.transcript.read_only || self.transcript.status == status {
            return false;
        }
        self.transcript.status = status;
        true
    }

    pub(crate) fn agent(&self) -> &str {
        if self.transcript.agent.is_empty() {
            &self.agent
        } else {
            &self.transcript.agent
        }
    }

    pub(crate) fn rejection(&self) -> Option<Rejection> {
        self.rejection
    }

    pub(crate) fn snapshots(&self) -> u64 {
        self.snapshots
    }

    /// Each entry with the revision it last changed at.
    pub(crate) fn keys(&self) -> Vec<(EntryId, u64)> {
        self.transcript
            .entries
            .iter()
            .map(|entry| {
                let revision = self.revisions.get(&entry.id).copied().unwrap_or(0);
                (entry.id, revision)
            })
            .collect()
    }

    /// The commands to send again on a new connection, in order. The
    /// program acknowledges one it already ran without running it twice.
    /// One that reached [`SENDS`] greeted connections, each of which ended
    /// without an answer, is given up and shown as refused.
    pub(crate) fn resend(&mut self) -> Vec<ClientMessage> {
        let before = self.pending.len();
        self.pending.retain(|pending| pending.sends < SENDS);
        if self.pending.len() < before {
            self.rejection = Some(Rejection::Unsupported);
        }
        self.pending
            .iter()
            .map(|pending| ClientMessage::Command {
                command: pending.command.clone(),
                id: pending.id,
            })
            .collect()
    }

    /// Records `command` as waiting and returns the message that sends it.
    pub(crate) fn submit(&mut self, command: Command) -> Result<ClientMessage> {
        if self.transcript.read_only {
            return Err(Error::ReadOnly);
        }
        if self.pending.len() >= MAX_PENDING {
            return Err(Error::Busy);
        }
        let id = CommandId::random();
        self.rejection = None;
        self.pending.push(Pending {
            command: command.clone(),
            id,
            // Out now on a greeted connection, else with the next one's resends.
            sends: u8::from(self.welcomed),
        });
        Ok(ClientMessage::Command { command, id })
    }

    /// Applies one message, returning whether anything shown changed. An
    /// error ends the connection; the session keeps what it had.
    pub(crate) fn receive(&mut self, message: ServerMessage) -> Result<bool> {
        match message {
            ServerMessage::Welcome { agent, version, .. } => {
                if version != VERSION {
                    return Err(Error::Version(version));
                }
                self.agent = agent;
                self.welcomed = true;
                // The resends went out right after the greeting.
                for pending in &mut self.pending {
                    pending.sends = pending.sends.saturating_add(1);
                }
                Ok(true)
            }
            _ if !self.welcomed => Err(Error::Handshake),
            ServerMessage::Snapshot { cursor, transcript } => {
                self.transcript = *transcript;
                self.cursor = Some(cursor);
                self.revisions.clear();
                self.snapshots += 1;
                Ok(true)
            }
            ServerMessage::Change { change, cursor } => {
                let follows = self.cursor.is_some_and(|after| {
                    after.epoch == cursor.epoch && after.seq.checked_add(1) == Some(cursor.seq)
                });
                if !follows {
                    return Err(Error::Stream {
                        after: self.cursor,
                        got: cursor,
                    });
                }
                self.transcript.apply(&change).map_err(Error::Apply)?;
                self.cursor = Some(cursor);
                self.touch(&change);
                Ok(true)
            }
            ServerMessage::Ack { id, rejected } => {
                self.pending.retain(|pending| pending.id != id);
                // Another client answering first is how a race ends, not
                // a failure worth showing.
                let shown = rejected.filter(|rejection| *rejection != Rejection::UnknownPermission);
                if shown.is_some() {
                    self.rejection = shown;
                }
                Ok(shown.is_some())
            }
            ServerMessage::Bye { reason } => Err(Error::Bye(reason)),
        }
    }

    fn touch(&mut self, change: &Change) {
        self.revision += 1;
        let id = match change {
            Change::Push { entry } | Change::Replace { entry } => entry.id,
            Change::Append { id, .. } => *id,
            Change::Truncate { .. } => {
                let entries = &self.transcript.entries;
                self.revisions
                    .retain(|id, _| entries.binary_search_by_key(id, |entry| entry.id).is_ok());
                return;
            }
            _ => return,
        };
        self.revisions.insert(id, self.revision);
    }
}
