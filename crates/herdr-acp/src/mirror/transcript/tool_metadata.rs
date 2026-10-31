//! Titles describe the submitted input, never a guessed action or outcome.

use std::borrow::Cow;

use herdr_chat_protocol::ToolKind;
use serde_json::Value;

use super::{TEXT, limited};
use crate::harness::out;

const TITLE: usize = 256;
const PATH: usize = 4096;

pub(super) fn name(name: &str) -> &str {
    name.strip_prefix("functions.")
        .or_else(|| name.strip_prefix("collaboration."))
        .unwrap_or(name)
}

/// JSON arguments may already be decoded, or encoded once in a native log.
pub(super) fn arguments(input: &Value) -> Cow<'_, Value> {
    if let Some(raw) = input.as_str().filter(|raw| raw.len() <= TEXT)
        && let Ok(value) = serde_json::from_str::<Value>(raw)
        && value.is_object()
    {
        return Cow::Owned(value);
    }
    Cow::Borrowed(input)
}

pub(super) fn describe(raw_name: &str, input: &Value) -> (ToolKind, String, Vec<String>) {
    let input = arguments(input);
    let field = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| input.get(key).and_then(Value::as_str))
            .filter(|value| !value.trim().is_empty())
    };
    let mut locations = Vec::new();
    let mut path = |keys: &[&str]| {
        let value = field(keys)?;
        // A truncated pathname would link somewhere else. Omit it instead.
        if value.len() <= PATH && !value.chars().any(char::is_control) {
            locations.push(value.to_owned());
        }
        Some(label(value))
    };
    let described = match name(raw_name) {
        "Bash" | "exec_command" | "shell_command" => (
            ToolKind::Execute,
            field(&["cmd", "command"]).map(|command| out::command_title(&limited(command, TEXT))),
        ),
        // Arbitrary JavaScript is opaque. Do not infer nested tools, commands,
        // files or outcomes from source strings that happen to mention them.
        "exec" => (ToolKind::Execute, Some("Tool script".into())),
        "write_stdin" => (
            ToolKind::Execute,
            identifier(input.get("session_id")).map(|id| {
                let action = if input
                    .get("chars")
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.is_empty())
                {
                    "Send input to"
                } else {
                    "Poll"
                };
                format!("{action} command session {id}")
            }),
        ),
        "wait" => (
            ToolKind::Execute,
            identifier(input.get("cell_id")).map(|id| {
                let action = if input.get("terminate").and_then(Value::as_bool) == Some(true) {
                    "Stop"
                } else {
                    "Wait for"
                };
                format!("{action} script cell {id}")
            }),
        ),
        "Read" | "read_file" | "view_image" => (ToolKind::Read, path(&["file_path", "path"])),
        "Write" | "write_file" => (
            ToolKind::Edit,
            path(&["file_path", "path"]).map(|path| format!("Write {path}")),
        ),
        "Edit" | "MultiEdit" | "NotebookEdit" => (
            ToolKind::Edit,
            path(&["file_path", "notebook_path", "path"]),
        ),
        "apply_patch" => (ToolKind::Edit, Some("Apply patch".into())),
        "Grep" | "Glob" => {
            let _ = path(&["path"]);
            let action = if name(raw_name) == "Glob" {
                "Files matching"
            } else {
                "For"
            };
            (
                ToolKind::Search,
                field(&["pattern"]).map(|pattern| format!("{action} {}", label(pattern))),
            )
        }
        "WebSearch" => (
            ToolKind::Search,
            field(&["query"]).map(|query| format!("The web for {}", label(query))),
        ),
        "WebFetch" => (ToolKind::Fetch, field(&["url"]).map(label)),
        "Task" | "Agent" => (
            ToolKind::Think,
            Some(field(&["description"]).map_or_else(
                || "Delegate task".into(),
                |description| format!("Delegate: {}", label(description)),
            )),
        ),
        "spawn_agent" => (
            ToolKind::Think,
            Some(field(&["task_name"]).map_or_else(
                || "Start agent".into(),
                |task| format!("Start agent: {}", label(task)),
            )),
        ),
        "send_message" | "followup_task" => {
            let action = if name(raw_name) == "send_message" {
                "Message"
            } else {
                "Continue"
            };
            (
                ToolKind::Think,
                Some(field(&["target"]).map_or_else(
                    || format!("{action} agent"),
                    |target| format!("{action} agent: {}", label(target)),
                )),
            )
        }
        "wait_agent" => (ToolKind::Think, Some("Wait for an agent update".into())),
        "list_agents" => (ToolKind::Think, Some("List agents".into())),
        "interrupt_agent" => (
            ToolKind::Think,
            Some(field(&["target"]).map_or_else(
                || "Interrupt agent".into(),
                |target| format!("Interrupt agent: {}", label(target)),
            )),
        ),
        "TaskOutput" => (ToolKind::Think, Some("Read task output".into())),
        "TaskStop" => (ToolKind::Think, Some("Stop task".into())),
        "TodoWrite" | "update_plan" => (ToolKind::Think, Some("Update plan".into())),
        "create_goal" => (ToolKind::Think, Some("Set goal".into())),
        "update_goal" => (ToolKind::Think, Some("Update goal".into())),
        "get_goal" => (ToolKind::Think, Some("Read goal".into())),
        // Async question provenance currently matches the original tool name.
        _ => (ToolKind::Other, None),
    };
    (
        described.0,
        label(described.1.as_deref().unwrap_or(raw_name)),
        locations,
    )
}

fn label(text: &str) -> String {
    limited(
        &limited(text, TITLE)
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .collect::<String>(),
        TITLE,
    )
}

pub(super) fn identifier(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(id) = value.as_u64() {
        return Some(id.to_string());
    }
    let id = value.as_str()?;
    (!id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)).then(|| id.to_owned())
}
