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
/// The longest session or cell id a title repeats.
const MAX_HANDLE: usize = 128;

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
    let target = field(&["target"]);
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
        // A script's source is opaque: nothing it mentions is known to run.
        "exec" => (ToolKind::Execute, Some("Tool script".to_owned())),
        "write_stdin" => {
            let typed = field(&["chars"]).is_some();
            let action = if typed { "Send input to" } else { "Poll" };
            (
                ToolKind::Execute,
                identifier(input.get("session_id"))
                    .map(|id| format!("{action} command session {id}")),
            )
        }
        "wait" => {
            let stop = input.get("terminate").and_then(Value::as_bool) == Some(true);
            let action = if stop { "Stop" } else { "Wait for" };
            (
                ToolKind::Execute,
                identifier(input.get("cell_id")).map(|id| format!("{action} script cell {id}")),
            )
        }
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
        "send_message" => (ToolKind::Think, Some(aimed("Message agent", target))),
        "followup_task" => (ToolKind::Think, Some(aimed("Continue agent", target))),
        "interrupt_agent" => (ToolKind::Think, Some(aimed("Interrupt agent", target))),
        "wait_agent" => (ToolKind::Think, Some("Wait for an agent update".to_owned())),
        "list_agents" => (ToolKind::Think, Some("List agents".to_owned())),
        "TaskOutput" => (ToolKind::Think, Some("Read task output".to_owned())),
        "TaskStop" => (ToolKind::Think, Some("Stop task".to_owned())),
        "TodoWrite" | "update_plan" => (ToolKind::Think, Some("Update plan".to_owned())),
        "create_goal" => (ToolKind::Think, Some("Set goal".to_owned())),
        "update_goal" => (ToolKind::Think, Some("Update goal".to_owned())),
        "get_goal" => (ToolKind::Think, Some("Read goal".to_owned())),
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

/// An action on another agent, which logs may leave unnamed.
fn aimed(action: &str, target: Option<&str>) -> String {
    target.map_or_else(|| action.to_owned(), |target| format!("{action}: {target}"))
}

/// A session or cell id, logged as a number or a short string.
fn identifier(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(id) = value.as_u64() {
        return Some(id.to_string());
    }
    let id = value.as_str()?;
    (!id.is_empty() && id.len() <= MAX_HANDLE && !id.chars().any(char::is_control))
        .then(|| id.to_owned())
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
