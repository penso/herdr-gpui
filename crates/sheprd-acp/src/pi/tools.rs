//! How pi's built-in tools read in the transcript.

use acp::v1 as wire;
use serde_json::Value;

use crate::harness::out;

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
    let diff = |old: Option<&str>, new: &str| {
        wire::ToolCallContent::Diff(
            wire::Diff::new(path.unwrap_or_default().to_owned(), new.to_owned())
                .old_text(old.map(str::to_owned)),
        )
    };
    let content: Vec<wire::ToolCallContent> = match name {
        "write" => vec![diff(None, field("content").unwrap_or_default())],
        "edit" => args
            .get("edits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|edit| {
                let text = |key: &str| edit.get(key).and_then(Value::as_str);
                diff(text("oldText"), text("newText").unwrap_or_default())
            })
            .collect(),
        // The whole command, so a long one is never hidden by its title.
        "bash" => field("command")
            .filter(|command| command.lines().nth(1).is_some())
            .map(out::text)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    };
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
    let content: Vec<wire::ToolCallContent> = result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| match block.get("type").and_then(Value::as_str) {
            Some("text") => block.get("text").and_then(Value::as_str).map(out::text),
            Some("image") => Some(wire::ToolCallContent::Content(wire::Content::new(
                wire::ContentBlock::Image(wire::ImageContent::new(
                    block.get("data")?.as_str()?,
                    block.get("mimeType")?.as_str()?,
                )),
            ))),
            _ => None,
        })
        .collect();
    fields.content(Some(content))
}
