//! The record Herdr UI continues this chat from once Herdr restores its pane
//! after a reboot or a server restart (`herdr_chat_protocol::restore`).
//!
//! It is written when the agent's session opens and removed when this
//! program ends on its own. A chat that is killed, as a reboot kills it,
//! leaves its record behind; that is the point.

use std::path::{Path, PathBuf};

use herdr_chat_protocol::{Agent, EndpointId, restore};

use crate::{cli::AgentChoice, herdr::Target};

/// One chat's record, waiting for the agent's session id.
#[derive(Clone, Debug)]
pub(crate) struct Keeper {
    adapter: bool,
    agent: Agent,
    cwd: PathBuf,
    dir: PathBuf,
    endpoint: EndpointId,
    pane: String,
    socket: PathBuf,
}

impl Keeper {
    /// A keeper for a chat a client could continue: a built-in agent, in a
    /// Herdr pane, listening for clients. A custom command, a chat outside
    /// Herdr, or one that does not listen keeps nothing.
    pub(crate) fn new(
        dir: Option<PathBuf>,
        target: Option<&Target>,
        endpoint: Option<&EndpointId>,
        agent: &AgentChoice,
        adapter: bool,
        cwd: &Path,
    ) -> Option<Self> {
        let AgentChoice::Builtin { agent, args } = agent else {
            return None;
        };
        // Extra agent arguments are not kept, so a chat started with them
        // could not be continued as it was.
        if !args.is_empty() {
            return None;
        }
        let target = target?;
        Some(Self {
            adapter,
            agent: *agent,
            cwd: cwd.to_owned(),
            dir: dir?,
            endpoint: endpoint?.clone(),
            pane: target.pane.clone(),
            socket: target.socket.clone(),
        })
    }

    /// The record of the chat whose agent session is `session`.
    fn record(&self, session: &str) -> restore::Record {
        restore::Record {
            adapter: self.adapter,
            agent: self.agent,
            cwd: self.cwd.clone(),
            endpoint: self.endpoint.clone(),
            pane: self.pane.clone(),
            session: session.to_owned(),
            socket: self.socket.clone(),
        }
    }

    /// Records the session that just opened, replacing an earlier one. A
    /// session id that could not be typed back safely is not kept, and its
    /// earlier record goes with it, so a restore never continues a session
    /// other than the chat's last.
    pub(crate) fn opened(&self, session: &str) {
        if self.record(session).write(&self.dir).is_err() {
            restore::remove(&self.dir, &self.endpoint);
        }
    }

    /// Removes the record: the chat ended on its own.
    pub(crate) fn ended(&self) {
        restore::remove(&self.dir, &self.endpoint);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
