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

/// Whether a finished call's card is about its input, not its output.
fn shows_input(name: &str, failed: bool) -> bool {
    !failed && matches!(name, "edit" | "write" | "read")
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
    if shows_input(name, failed) {
        return fields;
    }
    fields.content(content::result(
        result,
        MAX_ENTRY_TEXT,
        sheprd_chat_protocol::MAX_IMAGE_DATA,
    ))
}

/// A running call's live preview. pi's bash sends cumulative snapshots and
/// other tools have contracts of their own, so a partial result replaces the
/// last one rather than being appended to it.
#[derive(Debug)]
pub(super) struct Progress {
    /// What the call started with, within a preview's limit.
    base: Vec<wire::ToolCallContent>,
    /// The same within an entry's limit, which the finished card gets back.
    full: Vec<wire::ToolCallContent>,
    latest: Option<Vec<wire::ToolCallContent>>,
}

const BASE_TEXT: usize = MAX_ENTRY_TEXT / 4;
const PARTIAL_TEXT: usize = MAX_ENTRY_TEXT - BASE_TEXT;
/// A preview's images, kept tighter than a final result's.
const PARTIAL_MEDIA: usize = 1024 * 1024;

impl Progress {
    pub(super) fn new(name: &str, args: &Value) -> Self {
        Self {
            base: content::initial(name, args, BASE_TEXT),
            full: content::initial(name, args, MAX_ENTRY_TEXT),
            latest: None,
        }
    }

    /// Takes in a partial result; `false` when it changes nothing.
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

    /// What the finished call shows: its input again, whole when that fits,
    /// and the final result in place of the last partial one, which stays
    /// when the final result carries none.
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
        let media = sheprd_chat_protocol::MAX_IMAGE_DATA;
        let room = |input| MAX_ENTRY_TEXT.saturating_sub(content::size(input));
        let (whole, short) = (room(&self.full), room(&self.base));
        let output = content::result(result, MAX_ENTRY_TEXT, media);
        let fits = output
            .as_ref()
            .or(self.latest.as_ref())
            .is_none_or(|output| content::size(output) <= whole);
        // Both fit, or the input is what the card is for, or the output is:
        // a failure's reason must not give way to the command that caused it.
        let (input, limit) = if fits || shows_input(name, failed) {
            (self.full, whole)
        } else {
            (self.base, short)
        };
        let output = content::result(result, limit, media)
            .or(self.latest.filter(|latest| content::size(latest) <= limit))
            .unwrap_or_default();
        let content: Vec<_> = input.into_iter().chain(output).collect();
        wire::ToolCallUpdateFields::new()
            .status(Some(status))
            .content(Some(content))
    }
}
