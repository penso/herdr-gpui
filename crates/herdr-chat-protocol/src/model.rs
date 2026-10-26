//! The conversation a client draws, as the chat program projected it from ACP.
//!
//! Field order in these structs is not a wire contract: frames are JSON, read
//! by name, and every field a later version adds has a default.

use serde::{Deserialize, Serialize};

use crate::{Author, CommandId, Image};

mod question_answers;
pub use question_answers::{MAX_QUESTION_ANSWERS, MAX_QUESTION_TEXT, valid_question_answers};

/// An entry's identity, assigned by the program in increasing order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntryId(pub u64);

/// A permission request's identity, assigned by the program.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PermissionId(pub u64);

/// Everything a client needs to draw the conversation.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    /// The agent's display name, such as `Claude`.
    pub agent: String,
    /// Permission requests are answered with their allow option as they
    /// come, without asking.
    #[serde(default)]
    pub auto_accept: bool,
    /// How many attached clients watch, not counting the program's own view.
    #[serde(default)]
    pub clients: u32,
    /// Slash commands the agent offers.
    #[serde(default)]
    pub commands: Vec<SlashCommand>,
    #[serde(default)]
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub modes: Option<Modes>,
    /// The session's settings the agent lets a client change, such as the
    /// model and the thinking level.
    #[serde(default)]
    pub options: Vec<ConfigOption>,
    /// Permission requests still waiting for an answer, oldest first.
    #[serde(default)]
    pub permissions: Vec<Permission>,
    #[serde(default)]
    pub plan: Vec<PlanEntry>,
    /// Prompts waiting for the current turn to end, oldest first.
    #[serde(default)]
    pub queued: Vec<Queued>,
    /// A transcript viewer: prompts, permissions, and settings stay in the
    /// original agent terminal. The server refuses every command as well.
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub session: Option<SessionInfo>,
    #[serde(default)]
    pub status: Status,
    /// Older entries were dropped to keep the transcript bounded.
    #[serde(default)]
    pub truncated: bool,
    /// The prompt turn running now.
    #[serde(default)]
    pub turn: Option<Turn>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

impl Transcript {
    pub fn new(agent: impl Into<String>) -> Self {
        Self {
            agent: agent.into(),
            ..Self::default()
        }
    }

    pub fn entry(&self, id: EntryId) -> Option<&Entry> {
        self.position(id).map(|index| &self.entries[index])
    }

    pub fn last_entry_id(&self) -> Option<EntryId> {
        self.entries.last().map(|entry| entry.id)
    }

    /// The agent is waiting on the user to answer a permission request.
    pub fn is_blocked(&self) -> bool {
        !self.permissions.is_empty()
    }

    /// The first setting of `category`, such as the model.
    pub fn option(&self, category: OptionCategory) -> Option<&ConfigOption> {
        self.options
            .iter()
            .find(|option| option.category == category)
    }

    pub(crate) fn position(&self, id: EntryId) -> Option<usize> {
        self.entries
            .binary_search_by_key(&id, |entry| entry.id)
            .ok()
    }
}

/// Where the conversation is between turns.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum Status {
    /// The agent is starting or its session is being created.
    #[default]
    Starting,
    /// Waiting for the user's next prompt.
    Idle,
    /// A prompt turn is running.
    Working,
    /// The session could not start or the agent stopped answering.
    Failed { message: String },
    /// The agent process ended.
    Exited { message: String },
}

impl Status {
    pub fn accepts_prompts(&self) -> bool {
        matches!(self, Self::Idle | Self::Working | Self::Starting)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: EntryId,
    pub kind: EntryKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum EntryKind {
    /// Something the user said; prompts sent from any client, or replayed history.
    User {
        text: String,
        /// How many lines at the end of `text` name the files the prompt
        /// carried, each starting with [`ATTACHED`]; the user typed the rest.
        #[serde(default, skip_serializing_if = "is_zero")]
        files: usize,
    },
    /// The agent's answer, streamed in chunks.
    Agent {
        text: String,
    },
    /// The agent's reasoning, streamed in chunks.
    Thought {
        text: String,
    },
    /// A picture the user or the agent put in the conversation.
    Image {
        author: Author,
        image: Image,
    },
    Tool(Tool),
    /// How a prompt turn ended: how long it took and what it used.
    Turn(TurnSummary),
    /// Something the program says about the session, or an imported system event.
    Notice {
        /// Historical questions and answers, never actionable permission requests.
        #[serde(
            default,
            skip_serializing_if = "Vec::is_empty",
            deserialize_with = "question_answers::deserialize",
            serialize_with = "question_answers::serialize"
        )]
        answers: Vec<QuestionAnswer>,
        /// Optional source details, displayed only when the notice is expanded.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<String>,
        level: NoticeLevel,
        text: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuestionAnswer {
    pub question: String,
    pub answer: String,
}

impl EntryKind {
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::User { text, .. }
            | Self::Agent { text }
            | Self::Thought { text }
            | Self::Notice { text, .. } => Some(text),
            Self::Image { .. } | Self::Tool(_) | Self::Turn(_) => None,
        }
    }

    pub(crate) fn text_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::User { text, .. }
            | Self::Agent { text }
            | Self::Thought { text }
            | Self::Notice { text, .. } => Some(text),
            Self::Image { .. } | Self::Tool(_) | Self::Turn(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

/// A tool call, with everything its updates have said so far.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    /// The agent's identity for the call, unique within the session.
    pub call_id: String,
    #[serde(default)]
    pub content: Vec<ToolContent>,
    #[serde(default)]
    pub kind: ToolKind,
    /// Files the call touches.
    #[serde(default)]
    pub locations: Vec<String>,
    #[serde(default)]
    pub status: ToolStatus,
    pub title: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    SwitchMode,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ToolContent {
    Text {
        text: String,
    },
    /// A file change; `old` is absent for a new file.
    Diff {
        new: String,
        #[serde(default)]
        old: Option<String>,
        path: String,
    },
    /// A picture the tool returned, such as a screenshot or an image file read.
    Image {
        image: Image,
    },
    /// Output of a terminal the agent ran through the client; shown by id only.
    Terminal {
        id: String,
    },
}

/// A question the agent asked before running a tool call.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Permission {
    /// The tool call it is about, when the agent named one.
    #[serde(default)]
    pub call_id: Option<String>,
    pub id: PermissionId,
    pub options: Vec<PermissionOption>,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PermissionOption {
    pub id: String,
    #[serde(default)]
    pub kind: PermissionKind,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
    #[default]
    #[serde(other)]
    Other,
}

impl PermissionKind {
    pub fn allows(self) -> bool {
        matches!(self, Self::AllowOnce | Self::AllowAlways)
    }
}

impl Permission {
    /// The option auto-accepting picks: allowing this once, else always.
    /// None when the agent offers no way to allow it.
    pub fn allow_option(&self) -> Option<&PermissionOption> {
        [PermissionKind::AllowOnce, PermissionKind::AllowAlways]
            .into_iter()
            .find_map(|kind| self.options.iter().find(|option| option.kind == kind))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlanEntry {
    pub content: String,
    #[serde(default)]
    pub priority: PlanPriority,
    #[serde(default)]
    pub status: PlanStatus,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanPriority {
    High,
    Low,
    #[default]
    #[serde(other)]
    Medium,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    InProgress,
    Completed,
    #[default]
    #[serde(other)]
    Pending,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Modes {
    pub available: Vec<Mode>,
    pub current: String,
}

impl Modes {
    pub fn current(&self) -> Option<&Mode> {
        self.available.iter().find(|mode| mode.id == self.current)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Mode {
    #[serde(default)]
    pub description: Option<String>,
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Queued {
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub id: CommandId,
    pub text: String,
}

/// Starts each line the transcript adds after a prompt's text, one per file
/// the prompt carried.
pub const ATTACHED: &str = "\u{1f4ce} ";

fn is_zero(count: &usize) -> bool {
    *count == 0
}

/// The most files one prompt carries.
pub const MAX_ATTACHMENTS: usize = 10;

/// A file sent with a prompt, by its absolute path on the machine the
/// program runs on; the program reads it when the prompt goes out.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub path: String,
}

impl Attachment {
    /// The file's name, for chips and transcript lines.
    pub fn name(&self) -> &str {
        self.path
            .rsplit(['/', '\\'])
            .find(|part| !part.is_empty())
            .unwrap_or(&self.path)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionInfo {
    #[serde(default)]
    pub cwd: Option<String>,
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SlashCommand {
    #[serde(default)]
    pub description: String,
    /// What the command expects after its name, when it takes input.
    #[serde(default)]
    pub hint: Option<String>,
    pub name: String,
}

/// Context window use, in tokens, and what the session has cost so far.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub cost: Option<Cost>,
    pub size: u64,
    pub used: u64,
}

impl Usage {
    /// How full the context window is, from 0 to 100.
    pub fn percent(&self) -> u8 {
        if self.size == 0 {
            return 0;
        }
        let percent = self.used.saturating_mul(100) / self.size;
        u8::try_from(percent.min(100)).unwrap_or(100)
    }
}

/// An amount of money, in millionths of `currency`, so it compares exactly.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    pub currency: String,
    pub micros: u64,
}

/// A prompt turn that is running, since `started_ms` (Unix time).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub started_ms: u64,
}

/// How one prompt turn ended.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnSummary {
    pub duration_ms: u64,
    pub end: TurnEnd,
    /// Tokens the turn used, when the agent reports them.
    #[serde(default)]
    pub tokens: Option<Tokens>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnEnd {
    Done,
    Cancelled,
    /// The agent reached its output or turn limit.
    Limit,
    Refused,
    Failed,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Tokens {
    #[serde(default)]
    pub cached: u64,
    pub input: u64,
    pub output: u64,
    #[serde(default)]
    pub thought: u64,
    pub total: u64,
}

/// A setting of the session the agent lets clients change.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConfigOption {
    #[serde(default)]
    pub category: OptionCategory,
    #[serde(default)]
    pub description: Option<String>,
    pub id: String,
    pub kind: OptionKind,
    pub name: String,
}

impl ConfigOption {
    /// The name of the current value, for a button to show.
    pub fn current_name(&self) -> String {
        match &self.kind {
            OptionKind::Select { choices, current } => choices
                .iter()
                .find(|choice| &choice.id == current)
                .map_or_else(|| current.clone(), |choice| choice.name.clone()),
            OptionKind::Toggle { current } => if *current { "On" } else { "Off" }.to_owned(),
        }
    }

    /// Whether `value` is one this setting takes.
    pub fn accepts(&self, value: &OptionValue) -> bool {
        match (&self.kind, value) {
            (OptionKind::Select { choices, .. }, OptionValue::Choice { id }) => {
                choices.iter().any(|choice| &choice.id == id)
            }
            (OptionKind::Toggle { .. }, OptionValue::Toggle { .. }) => true,
            _ => false,
        }
    }
}

/// What a setting is about, so clients can place it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionCategory {
    /// How the agent asks for permission.
    Mode,
    Model,
    /// A further setting of the model, such as a speed or a variant.
    ModelConfig,
    /// How long the model thinks.
    ThoughtLevel,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum OptionKind {
    Select {
        choices: Vec<Choice>,
        current: String,
    },
    Toggle {
        current: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    #[serde(default)]
    pub description: Option<String>,
    /// The heading of the group the choice is listed under.
    #[serde(default)]
    pub group: Option<String>,
    pub id: String,
    pub name: String,
}

/// A new value for a setting.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum OptionValue {
    Choice { id: String },
    Toggle { value: bool },
}
