//! What clients ask for.

use acp::v1 as wire;
use herdr_chat_protocol::{
    Attachment, Author, Change, Command, CommandId, MAX_ATTACHMENTS, MAX_PROMPT_SIZE, OptionValue,
    PermissionId, Queued, Rejection, Status, Turn,
};

use super::{ConnId, Pending, Session, Update};

/// Prompts waiting behind a turn, and their text in bytes: the queue is
/// copied into every change and snapshot, so it stays well inside a frame.
const MAX_QUEUED: usize = 32;
const MAX_QUEUED_TEXT: usize = 1024 * 1024;

impl<R: FnMut(Update)> Session<R> {
    pub(super) fn command(&mut self, conn: ConnId, id: CommandId, command: Command) {
        if let Some((_, rejected)) = self.handled.iter().find(|(handled, _)| *handled == id) {
            let rejected = *rejected;
            return self.send_to(
                conn,
                herdr_chat_protocol::ServerMessage::Ack { id, rejected },
            );
        }
        let rejected = match command {
            Command::Prompt { text, attachments } => self.prompt(id, text, attachments),
            Command::Cancel => self.cancel(),
            Command::Answer { option, permission } => self.answer(permission, &option),
            Command::SetMode { id } => self.set_mode(id),
            Command::SetOption { id, value } => self.set_option(id, &value),
            Command::Unqueue { id } => self.unqueue(id),
            Command::SetAutoAccept { on } => {
                self.set_auto_accept(on);
                Ok(())
            }
        }
        .err();
        self.ack(conn, id, rejected);
    }

    fn prompt(
        &mut self,
        id: CommandId,
        text: String,
        attachments: Vec<Attachment>,
    ) -> Result<(), Rejection> {
        if text.trim().is_empty() && attachments.is_empty() {
            return Err(Rejection::EmptyPrompt);
        }
        if attachments.len() > MAX_ATTACHMENTS {
            return Err(Rejection::TooManyAttachments);
        }
        if !attachments.iter().all(crate::files::readable) {
            return Err(Rejection::UnknownFile);
        }
        if text.len() > MAX_PROMPT_SIZE {
            return Err(Rejection::PromptTooLong);
        }
        if !self.transcript.status.accepts_prompts() {
            return Err(Rejection::NotReady);
        }
        if self.turn.is_some() || !self.ready {
            let waiting = &self.transcript.queued;
            let text_size: usize = waiting.iter().map(|queued| queued.text.len()).sum();
            if waiting.len() >= MAX_QUEUED || text_size + text.len() > MAX_QUEUED_TEXT {
                return Err(Rejection::QueueFull);
            }
            let mut queued = self.transcript.queued.clone();
            queued.push(Queued {
                attachments,
                id,
                text,
            });
            self.queue_changed(queued);
            return Ok(());
        }
        self.send_prompt(&text, &attachments);
        Ok(())
    }

    /// Sends a prompt with its files, read now, and shows it in the
    /// transcript: the text with a line per file, then the pictures.
    fn send_prompt(&mut self, text: &str, attachments: &[Attachment]) {
        let Some(session) = self.acp_session.clone() else {
            return;
        };
        let prepared: Vec<_> = attachments
            .iter()
            .map(|attachment| crate::files::prepare(attachment, self.accepts))
            .collect();
        let mut shown = text.to_owned();
        for attachment in attachments {
            if !shown.is_empty() {
                shown.push('\n');
            }
            shown.push_str(herdr_chat_protocol::ATTACHED);
            shown.push_str(attachment.name());
        }
        let prompt = self.projector.prompt(&shown, attachments.len());
        self.emit(prompt);
        let mut blocks = Vec::with_capacity(prepared.len() + 1);
        if !text.trim().is_empty() {
            blocks.push(wire::ContentBlock::Text(wire::TextContent::new(text)));
        }
        for item in prepared {
            if let Some(image) = item.image {
                let change = self.projector.image(Author::User, image);
                self.emit(change);
            }
            blocks.push(item.block);
        }
        let request = wire::PromptRequest::new(session, blocks);
        if self.request("session/prompt", &request, Pending::Prompt) {
            // Only a prompt the agent received names the chat.
            (self.report)(Update::Prompt(shown));
            self.turn = Some(self.next_request);
            self.emit(Change::Turn {
                turn: Some(Turn {
                    started_ms: super::now_ms(),
                }),
            });
            self.status(Status::Working);
        }
    }

    /// Sends the oldest queued prompt once the agent is free.
    pub(super) fn next_queued(&mut self) {
        if self.turn.is_some() || !self.ready {
            return;
        }
        let mut queued = self.transcript.queued.clone();
        if queued.is_empty() {
            return;
        }
        let next = queued.remove(0);
        self.queue_changed(queued);
        self.send_prompt(&next.text, &next.attachments);
    }

    fn cancel(&mut self) -> Result<(), Rejection> {
        if self.turn.is_none() && self.transcript.queued.is_empty() {
            return Err(Rejection::NotWorking);
        }
        self.clear_queue();
        self.resolve_all();
        if self.turn.is_some()
            && let Some(session) = self.acp_session.clone()
            && let Err(error) = self
                .outbox
                .notify("session/cancel", &wire::CancelNotification::new(session))
        {
            self.fail(&error.to_string());
            return Err(Rejection::AgentUnavailable);
        }
        Ok(())
    }

    fn answer(&mut self, permission: PermissionId, option: &str) -> Result<(), Rejection> {
        let asked = self
            .asked
            .get(&permission)
            .ok_or(Rejection::UnknownPermission)?;
        if !asked.options.iter().any(|known| known == option) {
            return Err(Rejection::UnknownOption);
        }
        self.resolve(permission, Some(option));
        Ok(())
    }

    fn set_mode(&mut self, mode: String) -> Result<(), Rejection> {
        let session = self.acp_session.clone().ok_or(Rejection::NotReady)?;
        let known = self
            .transcript
            .modes
            .as_ref()
            .is_some_and(|modes| modes.available.iter().any(|known| known.id == mode));
        if !known {
            return Err(Rejection::UnknownMode);
        }
        let request = wire::SetSessionModeRequest::new(session, mode.clone());
        if !self.request("session/set_mode", &request, Pending::SetMode(mode)) {
            return Err(Rejection::AgentUnavailable);
        }
        Ok(())
    }

    fn set_option(&mut self, id: String, value: &OptionValue) -> Result<(), Rejection> {
        let session = self.acp_session.clone().ok_or(Rejection::NotReady)?;
        let known = self
            .transcript
            .options
            .iter()
            .any(|option| option.id == id && option.accepts(value));
        if !known {
            return Err(Rejection::UnknownSetting);
        }
        let request = wire::SetSessionConfigOptionRequest::new(
            session,
            id,
            crate::project::option_value(value),
        );
        if !self.request("session/set_config_option", &request, Pending::SetOption) {
            return Err(Rejection::AgentUnavailable);
        }
        Ok(())
    }

    /// Turns auto-accepting on or off; turning it on also answers the
    /// requests already waiting.
    fn set_auto_accept(&mut self, on: bool) {
        if self.transcript.auto_accept != on {
            self.emit(Change::AutoAccept { on });
        }
        if on {
            let waiting: Vec<PermissionId> = self
                .transcript
                .permissions
                .iter()
                .map(|permission| permission.id)
                .collect();
            for permission in waiting {
                self.auto_answer(permission);
            }
        }
    }

    /// Answers `permission` with its allow option while auto-accepting;
    /// one that offers no way to allow it keeps waiting for a person.
    pub(super) fn auto_answer(&mut self, permission: PermissionId) {
        if !self.transcript.auto_accept {
            return;
        }
        let option = self
            .transcript
            .permissions
            .iter()
            .find(|asked| asked.id == permission)
            .and_then(|asked| asked.allow_option())
            .map(|option| option.id.clone());
        if let Some(option) = option {
            self.resolve(permission, Some(&option));
        }
    }

    fn unqueue(&mut self, id: CommandId) -> Result<(), Rejection> {
        let mut queued = self.transcript.queued.clone();
        let index = queued
            .iter()
            .position(|queued| queued.id == id)
            .ok_or(Rejection::UnknownQueued)?;
        queued.remove(index);
        self.emit(Change::Queue { queued });
        Ok(())
    }
}
