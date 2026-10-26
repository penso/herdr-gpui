//! Bounded, pure projections of native Codex rollout and Claude JSONL records.
//! Logs are data: no links, tool calls, permissions, or files in them are run.

use std::{
    collections::{HashMap, HashSet},
    fs::{File, Metadata},
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    time::SystemTime,
};

use herdr_chat_protocol::{
    Author, ConfigOption, Entry, EntryId, EntryKind, OptionCategory, OptionKind, SessionInfo, Tool,
    ToolContent, ToolKind, ToolStatus, Transcript, Usage,
};
use serde_json::Value;

use super::{Error, Result};
use crate::{
    agents::Agent,
    session::{MAX_ENTRIES, MAX_TRANSCRIPT_TEXT, keep_from},
};

mod codex_messages;
mod notices;
mod question_replies;

use question_replies::QuestionKey;

const TAIL: u64 = 8 * 1024 * 1024;
const RECORD: usize = 1024 * 1024;
const TEXT: usize = 128 * 1024;
const BLOCKS: usize = 1024;
/// The longest session, record, or tool call id kept.
const MAX_ID: usize = 512;
/// The longest tool title or reported option value kept.
const MAX_TITLE: usize = 256;
/// The longest working directory kept.
const MAX_CWD: usize = 4096;

/// The agents whose native logs can be projected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptAgent {
    Claude,
    Codex,
}

impl From<TranscriptAgent> for Agent {
    fn from(agent: TranscriptAgent) -> Self {
        match agent {
            TranscriptAgent::Claude => Self::Claude,
            TranscriptAgent::Codex => Self::Codex,
        }
    }
}

impl TryFrom<Agent> for TranscriptAgent {
    /// The agent itself, which keeps no log this module reads.
    type Error = Agent;

    fn try_from(agent: Agent) -> Result<Self, Self::Error> {
        match agent {
            Agent::Claude => Ok(Self::Claude),
            Agent::Codex => Ok(Self::Codex),
            Agent::Copilot | Agent::Gemini | Agent::Opencode | Agent::Pi => Err(agent),
        }
    }
}

/// Who wrote a message record, read once from its native role or channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Role {
    User,
    Agent,
    Thought,
}

impl Role {
    /// The author of a native `user` or `assistant` record; other roles,
    /// such as system or developer messages, are not shown.
    fn of(role: &str) -> Option<Self> {
        match role {
            "user" => Some(Self::User),
            "assistant" => Some(Self::Agent),
            _ => None,
        }
    }

    fn author(self) -> Option<Author> {
        match self {
            Self::User => Some(Author::User),
            Self::Agent => Some(Author::Agent),
            Self::Thought => None,
        }
    }
}

pub(crate) struct Source {
    agent: TranscriptAgent,
    path: PathBuf,
    pinned: Option<File>,
    stamp: Option<Stamp>,
}

/// What a file looked like when it was last parsed. A file without a
/// modification time has no stamp and is parsed on every read.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: SystemTime,
}

impl Stamp {
    fn of(metadata: &Metadata) -> Option<Self> {
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok()?,
        })
    }
}

/// `metadata`, when it describes a regular file rather than a directory,
/// device, or FIFO.
fn regular(metadata: Metadata) -> Result<Metadata> {
    if metadata.is_file() {
        Ok(metadata)
    } else {
        Err(Error::NotRegularFile)
    }
}

impl Source {
    pub(crate) fn new(path: PathBuf, agent: TranscriptAgent) -> Self {
        Self {
            agent,
            path,
            pinned: None,
            stamp: None,
        }
    }

    /// An observer pins the file it verified, so replacing a pathname cannot
    /// swap another conversation into an already attached view.
    pub(crate) fn pinned(path: PathBuf, agent: TranscriptAgent, file: File) -> Self {
        Self {
            pinned: Some(file),
            ..Self::new(path, agent)
        }
    }

    pub(crate) fn read(&mut self) -> Result<Option<Transcript>> {
        let mut file = match &self.pinned {
            Some(file) => file.try_clone()?,
            None => {
                // Reject devices and FIFOs before opening, since opening a
                // FIFO can wait for another process indefinitely.
                regular(std::fs::metadata(&self.path)?)?;
                File::open(&self.path)?
            }
        };
        let metadata = regular(file.metadata()?)?;
        let stamp = Stamp::of(&metadata);
        if stamp.is_some() && self.stamp == stamp {
            return Ok(None);
        }
        // ponytail: reparse at most 8 MiB only when the file changes. An
        // incremental reader is warranted if large active logs make this hot.
        let mut offset = metadata.len().saturating_sub(TAIL);
        let mut partial = false;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset - 1))?;
            let mut previous = [0];
            file.read_exact(&mut previous)?;
            // Starting at a newline keeps the first complete record's ID.
            partial = previous[0] != b'\n';
        } else {
            file.seek(SeekFrom::Start(0))?;
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = Vec::new();
        file.take(TAIL + 1).read_to_end(&mut bytes)?;
        if partial {
            // The first record may start before the bounded window.
            let skip = bytes
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |end| end + 1);
            offset += skip as u64;
            bytes.drain(..skip);
        }
        let transcript = parse(self.agent, &bytes, offset);
        self.stamp = stamp;
        Ok(Some(transcript))
    }
}

/// A Codex `session_meta` payload's session id. The observer verifies a file
/// with this same reader, so both always agree on which session it holds.
pub(crate) fn codex_session(payload: &Value) -> Option<&str> {
    payload
        .get("id")
        .or_else(|| payload.get("session_id"))
        .and_then(Value::as_str)
}

fn limited(text: &str, limit: usize) -> String {
    let end = text.floor_char_boundary(limit.min(text.len()));
    text[..end].to_owned()
}

fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The content block types whose `text` is shown as the message's words.
const TEXT_BLOCKS: [&str; 4] = ["text", "input_text", "output_text", "summary_text"];

fn text_block(block: &Value) -> bool {
    TEXT_BLOCKS.contains(&string(block, "type"))
}

#[derive(Default)]
struct Projection {
    answered: HashMap<QuestionKey, herdr_chat_protocol::QuestionAnswer>,
    transcript: Transcript,
    tools: HashMap<String, usize>,
}

impl Projection {
    /// `text` cut to `limit` bytes, noting in the transcript when it was cut.
    fn clip(&mut self, text: &str, limit: usize) -> String {
        self.transcript.truncated |= text.len() > limit;
        limited(text, limit)
    }

    /// A record's words, a string or its text blocks one per line, within
    /// [`TEXT`].
    fn text(&mut self, value: &Value) -> String {
        if let Some(value) = value.as_str() {
            return self.clip(value, TEXT);
        }
        let Some(blocks) = value.as_array() else {
            return String::new();
        };
        self.transcript.truncated |= blocks.len() > BLOCKS;
        let mut out = String::new();
        for block in blocks.iter().take(BLOCKS).filter(|block| text_block(block)) {
            if out.len() >= TEXT {
                self.transcript.truncated = true;
                break;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            let room = TEXT.saturating_sub(out.len());
            out.push_str(&self.clip(string(block, "text"), room));
        }
        out
    }

    fn push(&mut self, id: EntryId, kind: EntryKind) {
        self.transcript.entries.push(Entry { id, kind });
    }

    fn message(&mut self, id: EntryId, role: Role, text: String) {
        if text.is_empty() {
            return;
        }
        let kind = match role {
            Role::User => EntryKind::User { text, files: 0 },
            Role::Agent => EntryKind::Agent { text },
            Role::Thought => EntryKind::Thought { text },
        };
        self.push(id, kind);
    }

    fn tool(&mut self, id: EntryId, call: &str, name: &str, input: &Value) {
        if call.is_empty() || name.is_empty() || call.len() > MAX_ID {
            return;
        }
        let input = input
            .as_str()
            .map_or_else(|| input.to_string(), str::to_owned);
        let tool = Tool {
            call_id: call.to_owned(),
            content: vec![ToolContent::Text {
                text: self.clip(&input, TEXT),
            }],
            kind: ToolKind::Other,
            locations: Vec::new(),
            status: ToolStatus::Pending,
            title: limited(name, MAX_TITLE),
        };
        self.tools
            .insert(call.to_owned(), self.transcript.entries.len());
        self.push(id, EntryKind::Tool(tool));
    }

    fn output(&mut self, call: &str, output: &Value, status: ToolStatus) {
        let Some(&index) = self.tools.get(call) else {
            return;
        };
        let output = self.text(output);
        let EntryKind::Tool(tool) = &mut self.transcript.entries[index].kind else {
            return;
        };
        tool.status = status;
        if !output.is_empty() {
            tool.content.truncate(1);
            tool.content.push(ToolContent::Text { text: output });
        }
    }

    fn codex(&mut self, id: EntryId, record: &Value) {
        let payload = &record["payload"];
        match string(record, "type") {
            "session_meta" => {
                let session = codex_session(payload).unwrap_or_default();
                self.session(session, string(payload, "cwd"));
            }
            "turn_context" => {
                self.reported_option(OptionCategory::Model, "model", string(payload, "model"));
                self.reported_option(
                    OptionCategory::ThoughtLevel,
                    "thinking",
                    string(payload, "effort"),
                );
            }
            "compacted" => {
                let message = string(payload, "message");
                self.transcript.truncated |= message.len() > TEXT;
                self.push(id, notices::compacted(message));
                if let Some(answers) = payload["retained_context"]["verified_answers"].as_array() {
                    self.transcript.truncated |= answers.len() >= BLOCKS;
                    for (index, answer) in answers.iter().take(BLOCKS - 1).enumerate() {
                        if let Some(notice) = self.retained_answer(answer, "order") {
                            self.push(EntryId(id.0 + index as u64 + 1), notice);
                        }
                    }
                }
            }
            "retained_context" if string(payload, "type") == "verified_answer" => {
                if let Some(notice) = self.retained_answer(payload, "acceptance_order") {
                    self.push(id, notice);
                }
            }
            "event_msg" if string(payload, "type") == "token_count" => {
                let info = &payload["info"];
                if let (Some(size), Some(used)) = (
                    info["model_context_window"]
                        .as_u64()
                        .filter(|size| *size > 0),
                    info["last_token_usage"]["total_tokens"].as_u64(),
                ) {
                    self.transcript.usage = Some(Usage {
                        cost: None,
                        size,
                        used,
                    });
                }
            }
            "response_item" => match string(payload, "type") {
                "message" => self.codex_message(id, record),
                "reasoning" => {
                    let summary = self.text(&payload["summary"]);
                    self.message(id, Role::Thought, summary);
                }
                "function_call" => self.tool(
                    id,
                    string(payload, "call_id"),
                    string(payload, "name"),
                    &payload["arguments"],
                ),
                "custom_tool_call" => self.tool(
                    id,
                    string(payload, "call_id"),
                    string(payload, "name"),
                    &payload["input"],
                ),
                "function_call_output" | "custom_tool_call_output" => self.output(
                    string(payload, "call_id"),
                    &payload["output"],
                    ToolStatus::Completed,
                ),
                _ => {}
            },
            // event_msg often repeats response_item text. Read one canonical
            // representation, never turn historical approval events into UI.
            _ => {}
        }
    }

    fn session(&mut self, id: &str, cwd: &str) {
        if id.is_empty() {
            return;
        }
        self.transcript.session = Some(SessionInfo {
            id: limited(id, MAX_ID),
            cwd: (!cwd.is_empty()).then(|| limited(cwd, MAX_CWD)),
            title: None,
        });
    }

    fn reported_option(&mut self, category: OptionCategory, id: &str, value: &str) {
        if value.is_empty() {
            return;
        }
        let option = ConfigOption {
            category,
            description: None,
            id: id.into(),
            // Empty choices expose only the reported value, never a setter.
            kind: OptionKind::Select {
                choices: Vec::new(),
                current: limited(value, MAX_TITLE),
            },
            name: id.into(),
        };
        if let Some(existing) = self
            .transcript
            .options
            .iter_mut()
            .find(|item| item.id == id)
        {
            *existing = option;
        } else {
            self.transcript.options.push(option);
        }
    }

    fn claude(&mut self, base: EntryId, record: &Value) {
        let Some(role) = Role::of(string(record, "type")) else {
            return;
        };
        if record["isSidechain"] == true {
            return;
        }
        self.session(string(record, "sessionId"), string(record, "cwd"));
        if role == Role::Agent {
            self.reported_option(
                OptionCategory::Model,
                "model",
                string(&record["message"], "model"),
            );
        }
        let content = &record["message"]["content"];
        if let Some(content) = content.as_str() {
            self.claude_message(base, role, record, content);
            return;
        }
        let Some(blocks) = content.as_array() else {
            return;
        };
        if blocks.len() > BLOCKS {
            self.transcript.truncated = true;
        }
        for (index, block) in blocks.iter().take(BLOCKS).enumerate() {
            let id = EntryId(base.0 + index as u64);
            match string(block, "type") {
                "text" => self.claude_message(id, role, record, string(block, "text")),
                "thinking" if role == Role::Agent => {
                    let thought = self.clip(string(block, "thinking"), TEXT);
                    self.message(id, Role::Thought, thought);
                }
                "tool_use" if role == Role::Agent => self.tool(
                    id,
                    string(block, "id"),
                    string(block, "name"),
                    &block["input"],
                ),
                "tool_result" if role == Role::User => self.output(
                    string(block, "tool_use_id"),
                    &block["content"],
                    if block["is_error"] == true {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Completed
                    },
                ),
                _ => {}
            }
        }
    }

    fn claude_message(&mut self, id: EntryId, role: Role, record: &Value, content: &str) {
        if role == Role::User
            && let Some(notice) = notices::project(record, content)
        {
            self.transcript.truncated |= content.len() > TEXT;
            self.push(id, notice);
        } else {
            let content = self.clip(content, TEXT);
            self.message(id, role, content);
        }
    }
}

fn parse(agent: TranscriptAgent, bytes: &[u8], mut offset: u64) -> Transcript {
    let mut projection = Projection {
        transcript: Transcript {
            agent: Agent::from(agent).display_name().to_owned(),
            read_only: true,
            truncated: offset > 0,
            ..Transcript::default()
        },
        ..Projection::default()
    };
    let mut seen = HashSet::new();
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        let start = offset;
        offset = offset.saturating_add(line.len() as u64);
        // A writer may be halfway through its final JSON record.
        if line.last() != Some(&b'\n') {
            break;
        }
        let record_limit = match agent {
            TranscriptAgent::Codex => TAIL as usize,
            TranscriptAgent::Claude => RECORD,
        };
        if line.len() > record_limit {
            projection.transcript.truncated = true;
            continue;
        }
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            projection.transcript.truncated |= line.len() > RECORD;
            continue;
        };
        if line.len() > RECORD && !codex_messages::image_record(&record) {
            projection.transcript.truncated = true;
            continue;
        }
        let Some(base) = start
            .checked_add(1)
            .and_then(|offset| offset.checked_mul(BLOCKS as u64))
        else {
            break;
        };
        match agent {
            TranscriptAgent::Codex => projection.codex(EntryId(base), &record),
            TranscriptAgent::Claude => {
                let uuid = string(&record, "uuid");
                if uuid.is_empty() || seen.insert(limited(uuid, MAX_ID)) {
                    projection.claude(EntryId(base), &record);
                }
            }
        }
    }
    let transcript = &mut projection.transcript;
    let keep = keep_from(&transcript.entries);
    if keep > 0 {
        transcript.entries.drain(..keep);
        transcript.truncated = true;
    }
    projection.transcript
}

#[cfg(test)]
mod tests;
