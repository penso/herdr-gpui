//! What a new chat starts in: the mode the user last picked for its agent,
//! Claude's "Bypass Permissions" say, and whether they left it auto-accepting.
//!
//! The choices belong to this client alone, like Overview marks: shared by
//! every window and kept across restarts. Only what the user picked counts,
//! so an agent leaving plan mode by itself changes nothing here, and only a
//! chat this app just started takes them. One restored, discovered, or
//! mirrored keeps the mode it has.

use super::{Chats, Target};
use crate::{i18n::Localize, state_file};
use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use sheprd_chat_protocol::{
    Command, EndpointId, OptionCategory, OptionKind, OptionValue, Status, Transcript,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

/// Agents kept; the least recently changed go first beyond this.
const MAX_AGENTS: usize = 32;
/// Bytes in an agent's name or a mode's id. A longer one is not kept, so
/// every choice the store accepts fits in the file it reads back.
const MAX_FIELD_BYTES: usize = 256;
/// Room for the most agents the store keeps, each as large as it accepts:
/// JSON escapes at most double a string's bytes, plus names and punctuation.
const MAX_FILE_BYTES: u64 = (MAX_AGENTS * (2 * 3 * MAX_FIELD_BYTES + 256) + 64) as u64;
const FILE: &str = "chat-last-modes.json";
/// How long a started chat may take to come up and still take the choices.
/// Past this it keeps the agent's own default, which asks more, not less.
const SETTLE: Duration = Duration::from_secs(120);

/// One agent's last choices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Last {
    agent: String,
    #[serde(default)]
    auto_accept: bool,
    /// The command that picked the mode, which is the agent's own mode
    /// setting or its older list of modes.
    #[serde(default)]
    mode: Option<Command>,
}

impl Last {
    /// Whether this fits the store's bounds and its mode picks a mode, so a
    /// damaged file cannot replay any other command.
    fn valid(&self) -> bool {
        let short = |text: &str| !text.is_empty() && text.len() <= MAX_FIELD_BYTES;
        short(&self.agent)
            && match &self.mode {
                None => true,
                Some(Command::SetMode { id }) => short(id),
                Some(Command::SetOption { id, value }) => {
                    short(id)
                        && match value {
                            OptionValue::Choice { id } => short(id),
                            OptionValue::Toggle { .. } => true,
                        }
                }
                Some(_) => false,
            }
    }

    /// What brings a chat showing `transcript` to these choices. A mode the
    /// agent no longer offers is left out.
    fn commands(&self, transcript: &Transcript) -> Vec<Command> {
        let mode = self
            .mode
            .as_ref()
            .filter(|command| differs(transcript, command))
            .cloned();
        let accept =
            (self.auto_accept != transcript.auto_accept).then_some(Command::SetAutoAccept {
                on: self.auto_accept,
            });
        mode.into_iter().chain(accept).collect()
    }
}

/// Whether the agent offers what `command` picks and is not already there.
fn differs(transcript: &Transcript, command: &Command) -> bool {
    match command {
        Command::SetMode { id } => transcript.modes.as_ref().is_some_and(|modes| {
            modes.current != *id && modes.available.iter().any(|mode| mode.id == *id)
        }),
        Command::SetOption { id, value } => transcript
            .options
            .iter()
            .find(|option| option.id == *id && option.category == OptionCategory::Mode)
            .is_some_and(|option| {
                let holds = match (&option.kind, value) {
                    (OptionKind::Select { current, .. }, OptionValue::Choice { id }) => {
                        current == id
                    }
                    (OptionKind::Toggle { current }, OptionValue::Toggle { value }) => {
                        current == value
                    }
                    _ => false,
                };
                option.accepts(value) && !holds
            }),
        _ => false,
    }
}

/// Whether `command` is a choice new chats follow.
pub(super) fn picked(transcript: &Transcript, command: &Command) -> bool {
    match command {
        Command::SetAutoAccept { .. } | Command::SetMode { .. } => true,
        Command::SetOption { id, .. } => transcript
            .options
            .iter()
            .any(|option| option.id == *id && option.category == OptionCategory::Mode),
        _ => false,
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    agents: Vec<Last>,
}

fn parse(bytes: &[u8]) -> serde_json::Result<Vec<Last>> {
    let mut agents = serde_json::from_slice::<Saved>(bytes)?.agents;
    agents.retain(Last::valid);
    let excess = agents.len().saturating_sub(MAX_AGENTS);
    agents.drain(..excess);
    Ok(agents)
}

/// The app's choices, least recently changed first.
#[derive(Default)]
pub(crate) struct LastModes {
    agents: Vec<Last>,
    quitting: bool,
    writer: Option<state_file::Writer<Saved>>,
}

impl Global for LastModes {}

impl LastModes {
    fn path() -> Option<PathBuf> {
        crate::preferences::state_dir().map(|dir| dir.join(FILE))
    }

    /// Called before starting GPUI. A missing or damaged file starts empty.
    pub(crate) fn load() -> Self {
        Self::at(Self::path())
    }

    fn at(path: Option<PathBuf>) -> Self {
        let bytes = path
            .as_deref()
            .map(|path| state_file::read(path, MAX_FILE_BYTES))
            .transpose()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "Cannot read the chats' last modes");
                None
            })
            .flatten();
        let agents = bytes
            .map(|bytes| parse(&bytes))
            .transpose()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "Cannot restore the chats' last modes");
                None
            })
            .unwrap_or_default();
        let writer =
            path.and_then(
                |path| match state_file::Writer::start("chat-last-modes", path) {
                    Ok(writer) => Some(writer),
                    Err(error) => {
                        tracing::warn!(%error, "Cannot start the chats' last modes worker");
                        None
                    }
                },
            );
        Self {
            agents,
            quitting: false,
            writer,
        }
    }

    pub(crate) fn install(self, cx: &mut App) {
        cx.set_global(self);
        cx.on_app_quit(|cx| {
            let last = cx.global_mut::<Self>();
            last.quitting = true;
            let writer = last.writer.take();
            cx.background_executor().spawn(async move {
                if let Some(writer) = writer {
                    writer.finish();
                }
            })
        })
        .detach();
    }

    /// Runs `f` against the app's choices, creating an unsaved set for
    /// fixtures that never installed them.
    pub(super) fn update<R>(cx: &mut App, f: impl FnOnce(&mut Self) -> R) -> R {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        f(cx.global_mut::<Self>())
    }

    /// The app's choices, or none for fixtures that never installed them.
    pub(super) fn of(cx: &App) -> Option<&Self> {
        cx.try_global::<Self>()
    }

    /// Records a command [`picked`] accepted for `agent`, moving it to the
    /// most recent end, and saves. A name or id too long to keep is refused.
    pub(super) fn remember(&mut self, agent: &str, command: Command) {
        let position = self.agents.iter().position(|last| last.agent == agent);
        let before = position.map(|index| self.agents[index].clone());
        let mut last = before.clone().unwrap_or_else(|| Last {
            agent: agent.to_owned(),
            auto_accept: false,
            mode: None,
        });
        match command {
            Command::SetAutoAccept { on } => last.auto_accept = on,
            command => last.mode = Some(command),
        }
        if !last.valid() || before.as_ref() == Some(&last) {
            return;
        }
        if let Some(index) = position {
            self.agents.remove(index);
        }
        self.agents.push(last);
        let excess = self.agents.len().saturating_sub(MAX_AGENTS);
        self.agents.drain(..excess);
        self.save();
    }

    /// What brings a new chat with `agent`, showing `transcript`, to the
    /// user's last choices.
    fn commands(&self, agent: &str, transcript: &Transcript) -> Vec<Command> {
        self.agents
            .iter()
            .find(|last| last.agent == agent)
            .map(|last| last.commands(transcript))
            .unwrap_or_default()
    }

    /// Writes what is pending and stops the worker, for tests to read the
    /// file back.
    #[cfg(test)]
    fn finish(&mut self) {
        if let Some(writer) = self.writer.take() {
            writer.finish();
        }
    }

    fn save(&self) {
        if self.quitting {
            return;
        }
        if let Some(writer) = &self.writer {
            writer.save(Saved {
                agents: self.agents.clone(),
            });
        }
    }
}

/// A chat this app started, as known before its pane attaches: a herdr-core
/// chat by the endpoint the window chose, a Herdr one by the pane its tab
/// made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Fresh {
    Endpoint(EndpointId),
    Pane(String),
}

impl Fresh {
    fn is(&self, target: &Target) -> bool {
        target.existing.is_none()
            && match self {
                Self::Endpoint(endpoint) => target.endpoint == *endpoint,
                Self::Pane(pane) => target.pane_id == *pane,
            }
    }
}

/// A started chat that has not taken the user's choices yet.
#[derive(Debug)]
pub(super) struct Awaited {
    chat: Fresh,
    until: Instant,
}

impl Chats {
    /// Marks `chat`, just started, to take the user's last choices once its
    /// session is up. The oldest gives way past the chats a window attaches.
    pub(super) fn expect_fresh(&mut self, chat: Fresh, now: Instant) {
        self.fresh.push(Awaited {
            chat,
            until: now + SETTLE,
        });
        let excess = self.fresh.len().saturating_sub(super::MAX_CHATS);
        self.fresh.drain(..excess);
    }

    /// Whether `chat` still waits for the user's choices.
    #[cfg(test)]
    pub(super) fn awaits(&self, chat: &Fresh) -> bool {
        self.fresh.iter().any(|awaited| awaited.chat == *chat)
    }

    /// Gives each started chat whose session came up the user's choices,
    /// once, and lets go of those that failed or took too long.
    pub(super) fn settle_fresh(&mut self, last: Option<&LastModes>, now: Instant) {
        if self.fresh.is_empty() {
            return;
        }
        let mut fresh = std::mem::take(&mut self.fresh);
        fresh.retain(|awaited| now < awaited.until && !self.settle(&awaited.chat, last));
        self.fresh = fresh;
    }

    /// Whether `chat` is done waiting. A session lists its modes before it
    /// first turns idle, and refuses a mode change while it starts.
    fn settle(&mut self, chat: &Fresh, last: Option<&LastModes>) -> bool {
        let Some(pane) = self.panes_mut().find(|pane| chat.is(&pane.target)) else {
            return false;
        };
        if !pane.session.live() {
            return false;
        }
        let transcript = pane.session.transcript();
        match transcript.status {
            Status::Starting => return false,
            Status::Failed { .. } | Status::Exited { .. } => return true,
            Status::Idle | Status::Working if transcript.read_only => return true,
            Status::Idle | Status::Working => {}
        }
        let commands = last
            .map(|last| last.commands(pane.session.agent(), transcript))
            .unwrap_or_default();
        for command in commands {
            if let Err(error) = pane.send(command) {
                pane.error = Some(error.localize());
                break;
            }
        }
        true
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
