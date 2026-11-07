//! Going back to before an earlier prompt: the agent forgets that prompt and
//! everything after it, the transcript drops them too, and the edited prompt
//! goes out in its place.
//!
//! The conversation only changes once the agent has gone back, so a failure
//! leaves the chat as it was. Clients get the shorter transcript as a new
//! epoch's snapshot, which replaces what they hold; the change log cannot
//! say it, as it only ever adds to a transcript or trims its oldest end.

use serde_json::Value;
use sheprd_chat_protocol::{
    Attachment, CommandId, Cursor, EntryId, EntryKind, NoticeLevel, Rejection, ServerMessage,
    Status, assist,
};

use super::{ConnId, Pending, Session, Update};
use crate::{harness::out, project, rpc};

/// The rewind under way, until the agent answers.
#[derive(Debug)]
pub(super) struct Rewinding {
    attachments: Vec<Attachment>,
    before: EntryId,
    conn: ConnId,
    id: CommandId,
    text: String,
}

/// The agent started again from an earlier point of its conversation.
pub(crate) struct Restarted {
    pub outbox: rpc::Outbox,
    /// The session to open in it.
    pub session: String,
}

/// Replaces the running agent with one that continues from `Rewind`'s point.
pub(crate) type Restart = Box<dyn FnMut(&out::Rewind) -> crate::Result<Restarted> + Send>;

impl<R: FnMut(Update)> Session<R> {
    /// Starts a rewind. `Ok` means it runs, and is acknowledged when it ends.
    pub(super) fn rewind(
        &mut self,
        conn: ConnId,
        id: CommandId,
        before: EntryId,
        text: String,
        attachments: Vec<Attachment>,
    ) -> Result<(), Rejection> {
        // A client that reconnected resends the command it has no answer for.
        if let Some(rewinding) = &mut self.rewinding {
            if rewinding.id != id {
                return Err(Rejection::NotReady);
            }
            rewinding.conn = conn;
            return Ok(());
        }
        if !self.rewinds {
            return Err(Rejection::CannotRewind);
        }
        self.check_prompt(&text, &attachments)?;
        if self.turn.is_some() {
            return Err(Rejection::TurnRunning);
        }
        let Some(session) = self.acp_session.clone().filter(|_| self.ready) else {
            return Err(Rejection::NotReady);
        };
        let rewind = self
            .target(before, session.0.to_string())
            .ok_or(Rejection::UnknownPrompt)?;
        self.rewinding = Some(Rewinding {
            attachments,
            before,
            conn,
            id,
            text,
        });
        self.ready = false;
        self.projector.close();
        self.status(Status::Starting);
        match self.config.restart.as_mut().map(|restart| restart(&rewind)) {
            None => {
                self.request(out::REWIND, &rewind, Pending::Rewind);
            }
            Some(Ok(restarted)) => {
                self.outbox = restarted.outbox;
                self.pending.clear();
                self.acp_session = None;
                self.config.load = Some(restarted.session);
                self.config.require_load = true;
                self.start();
            }
            // The agent still runs: it was not stopped for a prompt its log
            // does not have.
            Some(Err(crate::Error::Rewind(error))) => self.rewind_failed(&error.to_string()),
            Some(Err(error)) => self.fail(&error.to_string()),
        }
        Ok(())
    }

    /// What the agent must go back to for `before`, a prompt in the
    /// transcript: counted among the prompts of the same text from the end,
    /// so a prompt older than the transcript's oldest entry changes nothing,
    /// and an agent that logs more than was shown still finds it.
    fn target(&self, before: EntryId, session_id: String) -> Option<out::Rewind> {
        let prompt = |entry: &sheprd_chat_protocol::Entry| match &entry.kind {
            EntryKind::User { text, files } => Some(assist::typed(text, *files).to_owned()),
            _ => None,
        };
        let text = prompt(self.transcript.entry(before)?)?;
        let occurrence = self
            .transcript
            .entries
            .iter()
            .filter(|entry| entry.id >= before)
            .filter(|entry| prompt(entry).as_ref() == Some(&text))
            .count();
        Some(out::Rewind {
            occurrence,
            session_id,
            text,
        })
    }

    /// The agent's answer to [`out::REWIND`]: it may now be another session.
    pub(super) fn rewound(&mut self, result: Result<Value, acp::v1::Error>) {
        let value = match result {
            Ok(value) => value,
            Err(error) => return self.rewind_failed(&error.message),
        };
        if let Some(id) = value.get("sessionId").and_then(Value::as_str) {
            let id = acp::v1::SessionId::new(id.to_owned());
            (self.report)(Update::Opened(id.0.to_string()));
            let session = Some(self.session_info(&id));
            self.acp_session = Some(id);
            self.emit(sheprd_chat_protocol::Change::Session { session });
        }
        self.ready = true;
        self.status(Status::Idle);
        self.finish_rewind();
    }

    pub(super) fn session_info(
        &self,
        id: &acp::v1::SessionId,
    ) -> sheprd_chat_protocol::SessionInfo {
        sheprd_chat_protocol::SessionInfo {
            rewind: self.rewinds,
            ..project::session(id, &self.config.cwd)
        }
    }

    /// Drops the prompt and what followed it, then sends the edited one.
    pub(super) fn finish_rewind(&mut self) {
        let Some(rewinding) = self.rewinding.take() else {
            return;
        };
        let before = rewinding.before;
        let transcript = &mut self.transcript;
        let keep = transcript
            .entries
            .partition_point(|entry| entry.id < before);
        transcript.entries.truncate(keep);
        transcript.prompt_files.retain(|files| files.id < before);
        transcript.activities.clear();
        transcript.plan.clear();
        transcript.usage = None;
        self.projector.forget_from(before);
        let cursor = Cursor::new_epoch();
        self.epoch = cursor.epoch;
        self.seq = cursor.seq;
        self.log.clear();
        self.log_bytes = 0;
        let snapshot = ServerMessage::Snapshot {
            cursor,
            transcript: Box::new(self.transcript.clone()),
        };
        self.subscribers
            .retain(|subscriber| subscriber.outbox.try_send(snapshot.clone()).is_ok());
        self.ack(rewinding.conn, rewinding.id, None);
        self.send_prompt(&rewinding.text, &rewinding.attachments);
    }

    /// The agent stays where it was, and so does the conversation.
    fn rewind_failed(&mut self, message: &str) {
        self.notice(
            NoticeLevel::Warning,
            format!("Could not go back to that prompt: {message}"),
        );
        self.ready = true;
        self.status(Status::Idle);
        self.abandon_rewind();
        self.next_queued();
    }

    /// Refuses the rewind under way, if there is one.
    pub(super) fn abandon_rewind(&mut self) {
        if let Some(rewinding) = self.rewinding.take() {
            self.ack(rewinding.conn, rewinding.id, Some(Rejection::RewindFailed));
        }
    }
}
