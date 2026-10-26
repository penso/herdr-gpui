//! Turns ACP session updates into transcript changes.
//!
//! Pure: it reads the transcript the changes will apply to and keeps only the
//! bookkeeping ACP needs on top of it (which text entry is still streaming,
//! which entry holds which tool call).

use std::collections::HashMap;

use acp::v1 as wire;
use herdr_chat_protocol::{
    Author, Change, Choice, ConfigOption, Cost, Entry, EntryId, EntryKind, Image, Mode, Modes,
    NoticeLevel, OptionCategory, OptionKind, OptionValue, PlanEntry, PlanPriority, PlanStatus,
    SessionInfo, SlashCommand, Tokens, Tool, ToolContent, ToolKind, ToolStatus, Transcript,
    TurnSummary, Usage,
};

/// The most text one entry keeps; later chunks are dropped.
pub(crate) const MAX_ENTRY_TEXT: usize = 256 * 1024;
const TRUNCATED: &str = "\n… (truncated)";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Stream {
    User,
    Agent,
    Thought,
}

#[derive(Debug)]
struct Open {
    entry: EntryId,
    message: Option<String>,
    stream: Stream,
}

#[derive(Debug, Default)]
pub(crate) struct Projector {
    next: u64,
    open: Option<Open>,
    tools: HashMap<String, EntryId>,
}

impl Projector {
    fn next_id(&mut self) -> EntryId {
        self.next += 1;
        EntryId(self.next)
    }

    /// Gives the entry `old`, made but not yet in the transcript, a new id
    /// after every other, and returns it: entries were added before it.
    pub(crate) fn renumber(&mut self, old: EntryId) -> EntryId {
        let new = self.next_id();
        if let Some(open) = self.open.as_mut().filter(|open| open.entry == old) {
            open.entry = new;
        }
        for id in self.tools.values_mut().filter(|id| **id == old) {
            *id = new;
        }
        new
    }

    /// Ends the streaming entry, so the next chunk starts a new one.
    pub(crate) fn close(&mut self) {
        self.open = None;
    }

    pub(crate) fn notice(&mut self, level: NoticeLevel, text: impl Into<String>) -> Change {
        self.close();
        let id = self.next_id();
        Change::Push {
            entry: Entry {
                id,
                kind: EntryKind::Notice {
                    answers: Vec::new(),
                    details: None,
                    level,
                    text: bounded(text.into()),
                },
            },
        }
    }

    /// How a prompt turn ended.
    pub(crate) fn turn(&mut self, summary: TurnSummary) -> Change {
        self.close();
        let id = self.next_id();
        Change::Push {
            entry: Entry {
                id,
                kind: EntryKind::Turn(summary),
            },
        }
    }

    /// A picture of its own, between the text before and after it.
    pub(crate) fn image(&mut self, author: Author, image: Image) -> Change {
        self.close();
        let id = self.next_id();
        Change::Push {
            entry: Entry {
                id,
                kind: EntryKind::Image { author, image },
            },
        }
    }

    /// The user's own prompt, which ACP does not echo back, ending in
    /// `files` lines naming the files it carried.
    pub(crate) fn prompt(&mut self, text: &str, files: usize) -> Change {
        self.close();
        let id = self.next_id();
        Change::Push {
            entry: Entry {
                id,
                kind: EntryKind::User {
                    text: bounded(text.to_owned()),
                    files,
                },
            },
        }
    }

    pub(crate) fn text(
        &mut self,
        transcript: &Transcript,
        stream: Stream,
        message: Option<&str>,
        text: String,
    ) -> Option<Change> {
        if text.is_empty() {
            return None;
        }
        let open = self.open.as_ref().filter(|open| {
            open.stream == stream
                && open.message.as_deref() == message
                && transcript.last_entry_id() == Some(open.entry)
        });
        if let Some(open) = open {
            let have = transcript
                .entry(open.entry)
                .and_then(|entry| entry.kind.text())
                .map_or(0, str::len);
            let text = clip(have, text)?;
            return Some(Change::Append {
                id: open.entry,
                text,
            });
        }
        let id = self.next_id();
        self.open = Some(Open {
            entry: id,
            message: message.map(str::to_owned),
            stream,
        });
        let text = bounded(text);
        let kind = match stream {
            Stream::User => EntryKind::User { text, files: 0 },
            Stream::Agent => EntryKind::Agent { text },
            Stream::Thought => EntryKind::Thought { text },
        };
        Some(Change::Push {
            entry: Entry { id, kind },
        })
    }

    /// Records a tool call or an update to one, pushing it when it is new.
    pub(crate) fn tool(
        &mut self,
        transcript: &Transcript,
        call_id: &str,
        update: impl FnOnce(&mut Tool),
    ) -> Change {
        let known = self
            .tools
            .get(call_id)
            .and_then(|id| transcript.entry(*id))
            .and_then(|entry| match &entry.kind {
                EntryKind::Tool(tool) => Some((entry.id, tool.clone())),
                _ => None,
            });
        if let Some((id, mut tool)) = known {
            update(&mut tool);
            return Change::Replace {
                entry: Entry {
                    id,
                    kind: EntryKind::Tool(tool),
                },
            };
        }
        self.close();
        let id = self.next_id();
        self.tools.insert(call_id.to_owned(), id);
        let mut tool = Tool {
            call_id: call_id.to_owned(),
            content: Vec::new(),
            kind: ToolKind::Other,
            locations: Vec::new(),
            status: ToolStatus::Pending,
            title: call_id.to_owned(),
        };
        update(&mut tool);
        Change::Push {
            entry: Entry {
                id,
                kind: EntryKind::Tool(tool),
            },
        }
    }

    /// Forgets tool calls whose entries were dropped.
    pub(crate) fn forget_before(&mut self, before: EntryId) {
        self.tools.retain(|_, id| *id >= before);
    }

    pub(crate) fn update(
        &mut self,
        transcript: &Transcript,
        update: wire::SessionUpdate,
    ) -> Vec<Change> {
        use wire::SessionUpdate as U;
        let change = match update {
            U::UserMessageChunk(chunk) => self.chunk(transcript, Stream::User, chunk),
            U::AgentMessageChunk(chunk) => self.chunk(transcript, Stream::Agent, chunk),
            U::AgentThoughtChunk(chunk) => self.chunk(transcript, Stream::Thought, chunk),
            U::ToolCall(call) => {
                let call_id = call.tool_call_id.0.to_string();
                Some(self.tool(transcript, &call_id, |tool| apply_call(tool, call)))
            }
            U::ToolCallUpdate(update) => {
                let call_id = update.tool_call_id.0.to_string();
                Some(self.tool(transcript, &call_id, |tool| {
                    apply_fields(tool, update.fields);
                }))
            }
            U::Plan(plan) => Some(Change::Plan {
                plan: plan.entries.into_iter().map(plan_entry).collect(),
            }),
            U::AvailableCommandsUpdate(update) => Some(Change::Commands {
                commands: update
                    .available_commands
                    .into_iter()
                    .map(slash_command)
                    .collect(),
            }),
            U::CurrentModeUpdate(update) => {
                transcript.modes.as_ref().map(|_| Change::CurrentMode {
                    id: update.current_mode_id.0.to_string(),
                })
            }
            U::SessionInfoUpdate(update) => transcript.session.as_ref().map(|session| {
                let mut session = session.clone();
                update.title.update_to(&mut session.title);
                Change::Session {
                    session: Some(session),
                }
            }),
            U::UsageUpdate(update) => Some(Change::Usage {
                usage: Some(usage(&update)),
            }),
            U::Notice(notice) => {
                let level = match notice.severity {
                    wire::NoticeSeverity::Warning => NoticeLevel::Warning,
                    wire::NoticeSeverity::Error => NoticeLevel::Error,
                    _ => NoticeLevel::Info,
                };
                let text = match notice.description {
                    Some(description) => format!("{}\n{description}", notice.title),
                    None => notice.title,
                };
                Some(self.notice(level, text))
            }
            _ => None,
        };
        change.into_iter().collect()
    }

    fn chunk(
        &mut self,
        transcript: &Transcript,
        stream: Stream,
        chunk: wire::ContentChunk,
    ) -> Option<Change> {
        if let Some(image) = media(&chunk.content) {
            let author = match stream {
                Stream::User => Some(Author::User),
                Stream::Agent => Some(Author::Agent),
                Stream::Thought => None,
            };
            if let Some(author) = author {
                return Some(self.image(author, image));
            }
        }
        let message = chunk.message_id.map(|id| id.0.to_string());
        self.text(
            transcript,
            stream,
            message.as_deref(),
            block_text(&chunk.content),
        )
    }
}

pub(crate) fn modes(state: wire::SessionModeState) -> Modes {
    Modes {
        available: state
            .available_modes
            .into_iter()
            .map(|mode| Mode {
                description: mode.description,
                id: mode.id.0.to_string(),
                name: mode.name,
            })
            .collect(),
        current: state.current_mode_id.0.to_string(),
    }
}

/// Settings kept at most, and choices kept per setting.
const MAX_OPTIONS: usize = 64;
const MAX_CHOICES: usize = 256;

/// The settings in an ACP `configOptions` array. One this program cannot
/// read, such as a kind from a newer protocol, is skipped rather than
/// failing the message it came in.
pub(crate) fn options(value: Option<&serde_json::Value>) -> Vec<ConfigOption> {
    let Some(serde_json::Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| serde_json::from_value::<wire::SessionConfigOption>(item.clone()).ok())
        .filter_map(config_option)
        .take(MAX_OPTIONS)
        .collect()
}

fn config_option(option: wire::SessionConfigOption) -> Option<ConfigOption> {
    let kind = match option.kind {
        wire::SessionConfigKind::Select(select) => {
            let choice = |choice: wire::SessionConfigSelectOption, group: Option<&str>| Choice {
                description: choice.description.map(bounded),
                group: group.map(str::to_owned),
                id: choice.value.0.to_string(),
                name: bounded(choice.name),
            };
            let choices: Vec<Choice> = match select.options {
                wire::SessionConfigSelectOptions::Ungrouped(choices) => {
                    choices.into_iter().map(|c| choice(c, None)).collect()
                }
                wire::SessionConfigSelectOptions::Grouped(groups) => groups
                    .into_iter()
                    .flat_map(|group| {
                        let name = group.name;
                        group
                            .options
                            .into_iter()
                            .map(move |c| choice(c, Some(&name)))
                            .collect::<Vec<_>>()
                    })
                    .collect(),
                _ => return None,
            };
            OptionKind::Select {
                choices: choices.into_iter().take(MAX_CHOICES).collect(),
                current: select.current_value.0.to_string(),
            }
        }
        wire::SessionConfigKind::Boolean(toggle) => OptionKind::Toggle {
            current: toggle.current_value,
        },
        _ => return None,
    };
    Some(ConfigOption {
        category: match option.category {
            Some(wire::SessionConfigOptionCategory::Mode) => OptionCategory::Mode,
            Some(wire::SessionConfigOptionCategory::Model) => OptionCategory::Model,
            Some(wire::SessionConfigOptionCategory::ModelConfig) => OptionCategory::ModelConfig,
            Some(wire::SessionConfigOptionCategory::ThoughtLevel) => OptionCategory::ThoughtLevel,
            _ => OptionCategory::Other,
        },
        description: option.description.map(bounded),
        id: option.id.0.to_string(),
        kind,
        name: bounded(option.name),
    })
}

/// The ACP value for a setting's new value.
pub(crate) fn option_value(value: &OptionValue) -> wire::SessionConfigOptionValue {
    match value {
        OptionValue::Choice { id } => wire::SessionConfigOptionValue::value_id(id.clone()),
        OptionValue::Toggle { value } => wire::SessionConfigOptionValue::boolean(*value),
    }
}

/// Context use from an ACP usage update, with its cost in millionths.
pub(crate) fn usage(update: &wire::UsageUpdate) -> Usage {
    Usage {
        cost: update.cost.as_ref().and_then(|cost| {
            let micros = cost.amount * 1_000_000.0;
            // Saturating float-to-int casts; a negative or NaN amount is none.
            (micros.is_finite() && micros >= 0.0).then(|| Cost {
                currency: cost.currency.chars().take(8).collect(),
                micros: micros.round() as u64,
            })
        }),
        size: update.size,
        used: update.used,
    }
}

/// The token counts of a `session/prompt` result's `usage`, which ACP still
/// marks unstable, so it is read here rather than through the schema crate.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptUsage {
    #[serde(default)]
    cached_read_tokens: u64,
    input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    thought_tokens: u64,
    total_tokens: u64,
}

/// Tokens a prompt turn reported, when its result says.
pub(crate) fn tokens(result: &serde_json::Value) -> Option<Tokens> {
    let usage = serde_json::from_value::<PromptUsage>(result.get("usage")?.clone()).ok()?;
    Some(Tokens {
        cached: usage.cached_read_tokens,
        input: usage.input_tokens,
        output: usage.output_tokens,
        thought: usage.thought_tokens,
        total: usage.total_tokens,
    })
}

pub(crate) fn session(id: &wire::SessionId, cwd: &std::path::Path) -> SessionInfo {
    SessionInfo {
        cwd: Some(cwd.display().to_string()),
        id: id.0.to_string(),
        title: None,
    }
}

/// A content block's picture, when it is one every client can draw; any
/// other image stays a named placeholder.
fn media(block: &wire::ContentBlock) -> Option<Image> {
    match block {
        wire::ContentBlock::Image(image) => Image::new(&image.mime_type, image.data.clone()),
        _ => None,
    }
}

/// The readable form of a content block; media are named, not embedded.
pub(crate) fn block_text(block: &wire::ContentBlock) -> String {
    match block {
        wire::ContentBlock::Text(text) => text.text.clone(),
        wire::ContentBlock::Image(_) => "[image]".to_owned(),
        wire::ContentBlock::Audio(_) => "[audio]".to_owned(),
        wire::ContentBlock::ResourceLink(link) => format!("[{}]({})", link.name, link.uri),
        wire::ContentBlock::Resource(resource) => match &resource.resource {
            wire::EmbeddedResourceResource::TextResourceContents(text) => text.text.clone(),
            wire::EmbeddedResourceResource::BlobResourceContents(blob) => {
                format!("[{}]", blob.uri)
            }
            _ => "[resource]".to_owned(),
        },
        _ => "[content]".to_owned(),
    }
}

pub(crate) fn apply_call(tool: &mut Tool, call: wire::ToolCall) {
    tool.title = call.title;
    tool.kind = tool_kind(call.kind);
    tool.status = tool_status(call.status);
    tool.content = call.content.into_iter().map(tool_content).collect();
    tool.locations = locations(call.locations);
}

pub(crate) fn apply_fields(tool: &mut Tool, fields: wire::ToolCallUpdateFields) {
    if let Some(title) = fields.title {
        tool.title = title;
    }
    if let Some(kind) = fields.kind {
        tool.kind = tool_kind(kind);
    }
    if let Some(status) = fields.status {
        tool.status = tool_status(status);
    }
    if let Some(content) = fields.content {
        tool.content = content.into_iter().map(tool_content).collect();
    }
    if let Some(found) = fields.locations {
        tool.locations = locations(found);
    }
}

fn locations(locations: Vec<wire::ToolCallLocation>) -> Vec<String> {
    locations
        .into_iter()
        .map(|location| match location.line {
            Some(line) => format!("{}:{line}", location.path.display()),
            None => location.path.display().to_string(),
        })
        .collect()
}

fn tool_kind(kind: wire::ToolKind) -> ToolKind {
    match kind {
        wire::ToolKind::Read => ToolKind::Read,
        wire::ToolKind::Edit => ToolKind::Edit,
        wire::ToolKind::Delete => ToolKind::Delete,
        wire::ToolKind::Move => ToolKind::Move,
        wire::ToolKind::Search => ToolKind::Search,
        wire::ToolKind::Execute => ToolKind::Execute,
        wire::ToolKind::Think => ToolKind::Think,
        wire::ToolKind::Fetch => ToolKind::Fetch,
        wire::ToolKind::SwitchMode => ToolKind::SwitchMode,
        _ => ToolKind::Other,
    }
}

fn tool_status(status: wire::ToolCallStatus) -> ToolStatus {
    match status {
        wire::ToolCallStatus::InProgress => ToolStatus::InProgress,
        wire::ToolCallStatus::Completed => ToolStatus::Completed,
        wire::ToolCallStatus::Failed => ToolStatus::Failed,
        _ => ToolStatus::Pending,
    }
}

fn tool_content(content: wire::ToolCallContent) -> ToolContent {
    match content {
        wire::ToolCallContent::Content(content) => match media(&content.content) {
            Some(image) => ToolContent::Image { image },
            None => ToolContent::Text {
                text: bounded(block_text(&content.content)),
            },
        },
        wire::ToolCallContent::Diff(diff) => ToolContent::Diff {
            new: bounded(diff.new_text),
            old: diff.old_text.map(bounded),
            path: diff.path.display().to_string(),
        },
        wire::ToolCallContent::Terminal(terminal) => ToolContent::Terminal {
            id: terminal.terminal_id.0.to_string(),
        },
        _ => ToolContent::Text {
            text: "[content]".to_owned(),
        },
    }
}

fn plan_entry(entry: wire::PlanEntry) -> PlanEntry {
    PlanEntry {
        content: bounded(entry.content),
        priority: match entry.priority {
            wire::PlanEntryPriority::High => PlanPriority::High,
            wire::PlanEntryPriority::Low => PlanPriority::Low,
            _ => PlanPriority::Medium,
        },
        status: match entry.status {
            wire::PlanEntryStatus::InProgress => PlanStatus::InProgress,
            wire::PlanEntryStatus::Completed => PlanStatus::Completed,
            _ => PlanStatus::Pending,
        },
    }
}

fn slash_command(command: wire::AvailableCommand) -> SlashCommand {
    SlashCommand {
        description: command.description,
        hint: command.input.and_then(|input| match input {
            wire::AvailableCommandInput::Unstructured(input) => Some(input.hint),
            _ => None,
        }),
        name: command.name,
    }
}

/// `text`, cut to [`MAX_ENTRY_TEXT`] bytes on a character boundary.
pub(crate) fn bounded(text: String) -> String {
    clip(0, text).unwrap_or_default()
}

/// The part of `text` that fits after `have` bytes, marking where it was cut;
/// `None` once the entry is already full.
fn clip(have: usize, mut text: String) -> Option<String> {
    let room = MAX_ENTRY_TEXT.checked_sub(have)?;
    // A cut leaves at most three bytes free, so this also stops at a cut.
    if room < TRUNCATED.len() + 4 {
        return None;
    }
    if text.len() <= room {
        return Some(text);
    }
    let mut cut = room.saturating_sub(TRUNCATED.len());
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
    text.push_str(TRUNCATED);
    Some(text)
}

#[cfg(test)]
mod tests;
