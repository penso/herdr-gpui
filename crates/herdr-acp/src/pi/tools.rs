//! How pi's built-in tools read in the transcript.

use acp::v1 as wire;
use serde_json::Value;

use crate::{harness::out, project::MAX_ENTRY_TEXT};

mod content;

/// A call as it starts running.
pub(super) fn call(id: &str, name: &str, args: &Value) -> wire::ToolCall {
    let field = |key: &str| args.get(key).and_then(Value::as_str);
    let path = field("path");
    let (title, kind) = match name {
        "bash" => (
            out::command_title(field("command").unwrap_or_default()),
            wire::ToolKind::Execute,
        ),
        "read" => (
            format!("Read {}", path.unwrap_or("a file")),
            wire::ToolKind::Read,
        ),
        "write" => (
            format!("Write {}", path.unwrap_or("a file")),
            wire::ToolKind::Edit,
        ),
        "edit" => (
            format!("Edit {}", path.unwrap_or("a file")),
            wire::ToolKind::Edit,
        ),
        "grep" => (
            format!("Search for {}", field("pattern").unwrap_or_default()),
            wire::ToolKind::Search,
        ),
        "find" => (
            format!("Find {}", field("pattern").unwrap_or_default()),
            wire::ToolKind::Search,
        ),
        "ls" => (
            format!("List {}", path.unwrap_or(".")),
            wire::ToolKind::Read,
        ),
        _ => (name.to_owned(), wire::ToolKind::Other),
    };
    let content = content::initial(name, args, MAX_ENTRY_TEXT);
    let mut call = wire::ToolCall::new(id.to_owned(), title)
        .kind(kind)
        .status(wire::ToolCallStatus::InProgress)
        .content(content)
        .raw_input(args.clone());
    if let Some(path) = path {
        call = call.locations(vec![wire::ToolCallLocation::new(path.to_owned())]);
    }
    call
}

/// What a finished call adds: its output, except for an edit, whose diff
/// already shows, unless it failed.
pub(super) fn finished(name: &str, result: &Value, failed: bool) -> wire::ToolCallUpdateFields {
    let status = if failed {
        wire::ToolCallStatus::Failed
    } else {
        wire::ToolCallStatus::Completed
    };
    let fields = wire::ToolCallUpdateFields::new().status(Some(status));
    if !failed && matches!(name, "edit" | "write" | "read") {
        return fields;
    }
    fields.content(content::result(
        result,
        MAX_ENTRY_TEXT,
        herdr_chat_protocol::MAX_IMAGE_DATA,
    ))
}

/// Bounded live preview. Pi's built-in bash sends cumulative snapshots; custom
/// tools have their own contracts, so show their latest partial result without
/// guessing that it is a delta to append.
#[derive(Debug)]
pub(super) struct Progress {
    base: Vec<wire::ToolCallContent>,
    latest: Option<Vec<wire::ToolCallContent>>,
}

const BASE_TEXT: usize = MAX_ENTRY_TEXT / 4;
const PARTIAL_TEXT: usize = MAX_ENTRY_TEXT - BASE_TEXT;
// Preview images have a tighter limit than authoritative final tool results.
const PARTIAL_MEDIA: usize = 1024 * 1024;

impl Progress {
    pub(super) fn new(name: &str, args: &Value) -> Self {
        Self {
            base: content::initial(name, args, BASE_TEXT),
            latest: None,
        }
    }

    pub(super) fn replace(&mut self, result: &Value) -> bool {
        let Some(content) = content::result(result, PARTIAL_TEXT, PARTIAL_MEDIA) else {
            return false;
        };
        if self.latest.as_ref() == Some(&content) {
            return false;
        }
        self.latest = Some(content);
        true
    }

    pub(super) fn fields(&self) -> wire::ToolCallUpdateFields {
        let content = self
            .base
            .iter()
            .chain(self.latest.iter().flatten())
            .cloned()
            .collect();
        wire::ToolCallUpdateFields::new()
            .status(Some(wire::ToolCallStatus::InProgress))
            .content(Some(content))
    }

    pub(super) fn finish(
        self,
        name: &str,
        result: &Value,
        failed: bool,
    ) -> wire::ToolCallUpdateFields {
        if self.latest.is_none() {
            return finished(name, result, failed);
        }
        let status = if failed {
            wire::ToolCallStatus::Failed
        } else {
            wire::ToolCallStatus::Completed
        };
        let output = content::result(result, PARTIAL_TEXT, herdr_chat_protocol::MAX_IMAGE_DATA)
            .or(self.latest);
        wire::ToolCallUpdateFields::new()
            .status(Some(status))
            .content(output.map(|output| self.base.into_iter().chain(output).collect()))
    }
}
