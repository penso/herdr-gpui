//! What a client of an earlier protocol revision is sent instead of a change
//! it cannot decode, which would otherwise end its connection.
//!
//! Every change moves the client's cursor by one, and a client takes a
//! change only right after the one before it, so an unknown change cannot
//! simply be left out. It is replaced with one the client knows that alters
//! nothing: the attached-client count, as it already stands.

use herdr_chat_protocol::{Change, REVISION, ServerMessage};

#[derive(Debug)]
pub(crate) struct Downgrade {
    /// The attached-client count the client holds, known from its snapshot.
    clients: Option<u32>,
    revision: u32,
}

impl Downgrade {
    pub(crate) fn new(revision: u32) -> Self {
        Self {
            clients: None,
            revision,
        }
    }

    /// Whether the client reads everything this program sends.
    pub(crate) fn current(&self) -> bool {
        self.revision >= REVISION
    }

    /// `message` as the client can read it, or `None` when it cannot be put
    /// so, which ends the connection; the client then reconnects for a
    /// snapshot.
    pub(crate) fn message(&mut self, message: ServerMessage) -> Option<ServerMessage> {
        if self.current() {
            return Some(message);
        }
        match message {
            ServerMessage::Snapshot { cursor, transcript } => {
                self.clients = Some(transcript.clients);
                Some(ServerMessage::Snapshot { cursor, transcript })
            }
            ServerMessage::Change { change, cursor } => {
                let change = match change {
                    Change::Clients { count } => {
                        self.clients = Some(count);
                        Change::Clients { count }
                    }
                    Change::Activity { .. } => Change::Clients {
                        count: self.clients?,
                    },
                    change => change,
                };
                Some(ServerMessage::Change { change, cursor })
            }
            message => Some(message),
        }
    }
}

#[cfg(test)]
mod tests;
