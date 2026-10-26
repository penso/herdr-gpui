//! One step from one transcript to the next, and the reducer every side runs.

use serde::{Deserialize, Serialize};

use crate::{
    ConfigOption, Entry, EntryId, EntryKind, Error, Modes, Permission, PermissionId, PlanEntry,
    Queued, Result, SessionInfo, SlashCommand, Status, Transcript, Turn, Usage,
    valid_question_answers,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "change")]
pub enum Change {
    /// A new entry after every existing one.
    Push {
        entry: Entry,
    },
    /// More text for a user, agent, thought, or notice entry.
    Append {
        id: EntryId,
        text: String,
    },
    /// An entry's new content, such as a tool call after an update.
    Replace {
        entry: Entry,
    },
    /// Entries before `before` were dropped to keep the transcript bounded.
    Truncate {
        before: EntryId,
    },
    Status {
        status: Status,
    },
    PermissionAsked {
        permission: Permission,
    },
    /// The request was answered or withdrawn, from whichever client.
    PermissionResolved {
        id: PermissionId,
    },
    Plan {
        plan: Vec<PlanEntry>,
    },
    Modes {
        modes: Option<Modes>,
    },
    CurrentMode {
        id: String,
    },
    Options {
        options: Vec<ConfigOption>,
    },
    /// A prompt turn started, or the running one ended.
    Turn {
        turn: Option<Turn>,
    },
    Queue {
        queued: Vec<Queued>,
    },
    Clients {
        count: u32,
    },
    Session {
        session: Option<SessionInfo>,
    },
    Commands {
        commands: Vec<SlashCommand>,
    },
    Usage {
        usage: Option<Usage>,
    },
    /// Permission requests are now answered without asking, or asked again.
    AutoAccept {
        on: bool,
    },
}

impl Transcript {
    /// Applies `change`, or leaves the transcript as it was and reports why it
    /// does not fit.
    pub fn apply(&mut self, change: &Change) -> Result<()> {
        if let Change::Push { entry } | Change::Replace { entry } = change
            && let EntryKind::Notice { answers, .. } = &entry.kind
            && !valid_question_answers(answers)
        {
            return Err(Error::QuestionAnswersLimit(entry.id));
        }
        match change {
            Change::Push { entry } => {
                if let Some(last) = self.last_entry_id()
                    && entry.id <= last
                {
                    return Err(Error::EntryOrder { id: entry.id, last });
                }
                self.entries.push(entry.clone());
            }
            Change::Append { id, text } => {
                let index = self.position(*id).ok_or(Error::UnknownEntry(*id))?;
                self.entries[index]
                    .kind
                    .text_mut()
                    .ok_or(Error::NotText(*id))?
                    .push_str(text);
            }
            Change::Replace { entry } => {
                let index = self
                    .position(entry.id)
                    .ok_or(Error::UnknownEntry(entry.id))?;
                self.entries[index] = entry.clone();
            }
            Change::Truncate { before } => {
                let keep = self.entries.partition_point(|entry| entry.id < *before);
                if keep > 0 {
                    self.entries.drain(..keep);
                    self.truncated = true;
                }
            }
            Change::Status { status } => self.status = status.clone(),
            Change::PermissionAsked { permission } => {
                if self
                    .permissions
                    .iter()
                    .any(|known| known.id == permission.id)
                {
                    return Err(Error::DuplicatePermission(permission.id));
                }
                self.permissions.push(permission.clone());
            }
            Change::PermissionResolved { id } => {
                let index = self
                    .permissions
                    .iter()
                    .position(|permission| permission.id == *id)
                    .ok_or(Error::UnknownPermission(*id))?;
                self.permissions.remove(index);
            }
            Change::Plan { plan } => self.plan.clone_from(plan),
            Change::Modes { modes } => self.modes.clone_from(modes),
            Change::CurrentMode { id } => {
                self.modes
                    .as_mut()
                    .ok_or(Error::NoModes)?
                    .current
                    .clone_from(id);
            }
            Change::Options { options } => self.options.clone_from(options),
            Change::Turn { turn } => self.turn = *turn,
            Change::Queue { queued } => self.queued.clone_from(queued),
            Change::Clients { count } => self.clients = *count,
            Change::Session { session } => self.session.clone_from(session),
            Change::Commands { commands } => self.commands.clone_from(commands),
            Change::Usage { usage } => self.usage.clone_from(usage),
            Change::AutoAccept { on } => self.auto_accept = *on,
        }
        Ok(())
    }
}
