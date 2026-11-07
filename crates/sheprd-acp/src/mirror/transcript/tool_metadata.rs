//! What a logged tool call reads as: logs keep only a tool's name and its
//! arguments, so the known tools' fields give the card what a live agent
//! reports itself. Every other tool keeps its name.

use std::borrow::Cow;

use serde_json::Value;
use sheprd_chat_protocol::ToolKind;

use super::{MAX_TITLE, TEXT, limited};
use crate::harness::out;

/// The longest path listed as a location: a cut one would name another file.
const MAX_PATH: usize = 4096;

/// A logged call's kind, title, and the files it names.
pub(super) struct Described {
    pub(super) kind: ToolKind,
    pub(super) locations: Vec<String>,
    pub(super) title: String,
}

/// Codex logs its arguments encoded as a string.
fn arguments(input: &Value) -> Cow<'_, Value> {
    input
        .as_str()
        .filter(|raw| raw.len() <= TEXT)
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .filter(Value::is_object)
        .map_or(Cow::Borrowed(input), Cow::Owned)
}

pub(super) fn describe(name: &str, input: &Value) -> Described {
    let input = arguments(input);
    let field = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| input.get(*key).and_then(Value::as_str))
            .filter(|value| !value.trim().is_empty())
    };
    let path = field(&["file_path", "notebook_path", "path"]);
    let titled = |verb: &str, subject: Option<&str>| subject.map(|text| format!("{verb} {text}"));
    let short = name
        .strip_prefix("functions.")
        .or_else(|| name.strip_prefix("collaboration."))
        .unwrap_or(name);
    let (kind, title) = match short {
        "Bash" | "exec_command" | "shell" | "shell_command" => (
            ToolKind::Execute,
            field(&["cmd", "command"]).map(out::command_title),
        ),
        "Read" | "read_file" | "view_image" => (ToolKind::Read, titled("Read", path)),
        "Write" | "write_file" => (ToolKind::Edit, titled("Write", path)),
        "Edit" | "MultiEdit" | "NotebookEdit" => (ToolKind::Edit, titled("Edit", path)),
        "apply_patch" => (ToolKind::Edit, Some("Apply patch".to_owned())),
        "Grep" => (ToolKind::Search, titled("Search for", field(&["pattern"]))),
        "Glob" => (ToolKind::Search, titled("Find", field(&["pattern"]))),
        "WebSearch" => (
            ToolKind::Search,
            titled("Search the web for", field(&["query"])),
        ),
        "WebFetch" => (ToolKind::Fetch, titled("Fetch", field(&["url"]))),
        "Task" | "Agent" | "spawn_agent" => (
            ToolKind::Subagent,
            field(&["description", "task_name"]).map(str::to_owned),
        ),
        _ => return unknown(name),
    };
    Described {
        kind,
        locations: path
            .filter(|path| path.len() <= MAX_PATH && !path.chars().any(char::is_control))
            .map(str::to_owned)
            .into_iter()
            .collect(),
        title: one_line(title.as_deref().unwrap_or(name)),
    }
}

fn unknown(name: &str) -> Described {
    Described {
        kind: ToolKind::Other,
        locations: Vec::new(),
        title: limited(name, MAX_TITLE),
    }
}

fn one_line(text: &str) -> String {
    let line: String = limited(text, MAX_TITLE)
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    limited(&line, MAX_TITLE)
}

#[cfg(test)]
mod tests;
