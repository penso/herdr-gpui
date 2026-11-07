//! Codex's thread items as tool calls: a title, a kind, and what each shows.

use acp::v1 as wire;
use serde_json::Value;

use crate::harness::out;

/// How a started item shows, or `None` for one that is not a call (a
/// message, reasoning, the user's own prompt).
pub(crate) fn started(item: &Value) -> Option<wire::ToolCall> {
    let id = item.get("id")?.as_str()?;
    let (title, kind, content) = describe(item)?;
    let mut call = wire::ToolCall::new(id.to_owned(), title)
        .kind(kind)
        .status(wire::ToolCallStatus::InProgress)
        .content(content)
        .raw_input(item.clone());
    let paths = paths(item);
    if !paths.is_empty() {
        call = call.locations(paths.into_iter().map(wire::ToolCallLocation::new).collect());
    }
    Some(call)
}

/// How a finished item updates its call: its status, and its output.
pub(crate) fn completed(item: &Value) -> Option<wire::ToolCallUpdate> {
    let id = item.get("id")?.as_str()?;
    let (title, _, content) = describe(item)?;
    let status = match item.get("status").and_then(Value::as_str) {
        Some("failed" | "declined") => wire::ToolCallStatus::Failed,
        Some("inProgress") => wire::ToolCallStatus::InProgress,
        _ => wire::ToolCallStatus::Completed,
    };
    let mut content = content;
    if let Some(output) = output(item) {
        content.push(out::text(&output));
    }
    let fields = wire::ToolCallUpdateFields::new()
        .title(Some(title))
        .status(Some(status))
        .content(Some(content));
    Some(wire::ToolCallUpdate::new(id.to_owned(), fields))
}

fn describe(item: &Value) -> Option<(String, wire::ToolKind, Vec<wire::ToolCallContent>)> {
    let field = |key: &str| item.get(key).and_then(Value::as_str);
    Some(match field("type")? {
        "commandExecution" => (
            out::command_title(field("command").unwrap_or_default()),
            wire::ToolKind::Execute,
            // The whole command, so what an approval runs is never hidden.
            field("command")
                .filter(|command| command.lines().nth(1).is_some())
                .map(out::text)
                .into_iter()
                .collect(),
        ),
        "fileChange" => {
            let changes = changes(item);
            let title = match changes.as_slice() {
                [(path, _)] => format!("Edit {path}"),
                changes => format!("Edit {} files", changes.len()),
            };
            let content = changes
                .iter()
                .map(|(path, diff)| out::text(&format!("{path}\n{diff}")))
                .collect();
            (title, wire::ToolKind::Edit, content)
        }
        "mcpToolCall" => (
            format!(
                "{}: {}",
                field("server").unwrap_or("tool"),
                field("tool").unwrap_or_default()
            ),
            wire::ToolKind::Other,
            Vec::new(),
        ),
        "dynamicToolCall" => (
            field("tool").unwrap_or("Tool").to_owned(),
            wire::ToolKind::Other,
            Vec::new(),
        ),
        "webSearch" => (
            field("query").map_or_else(
                || "Search the web".to_owned(),
                |query| format!("Search the web for {query}"),
            ),
            wire::ToolKind::Search,
            Vec::new(),
        ),
        "imageView" => (
            format!("View {}", field("path").unwrap_or("an image")),
            wire::ToolKind::Read,
            Vec::new(),
        ),
        "collabAgentToolCall" => (collab_title(item), wire::ToolKind::Think, Vec::new()),
        _ => return None,
    })
}

/// A collab call's title: a sub-agent's task, or what the call does to one.
/// (`subAgentActivity` items are not calls: they only report how a
/// sub-agent stands.)
fn collab_title(item: &Value) -> String {
    let field = |key: &str| item.get(key).and_then(Value::as_str);
    let prompt = field("prompt")
        .and_then(|prompt| prompt.lines().find(|line| !line.trim().is_empty()))
        .map(str::trim);
    match (field("tool"), prompt) {
        (Some("spawnAgent"), Some(prompt)) => prompt.to_owned(),
        (Some("spawnAgent"), None) => "Start a sub-agent".to_owned(),
        (Some("wait"), _) => "Wait for sub-agents".to_owned(),
        (Some("sendInput" | "sendMessage" | "followupTask"), _) => "Message a sub-agent".to_owned(),
        (Some("closeAgent"), _) => "Close a sub-agent".to_owned(),
        (Some("interruptAgent"), _) => "Interrupt a sub-agent".to_owned(),
        (Some("resumeAgent"), _) => "Resume a sub-agent".to_owned(),
        (Some("listAgents"), _) => "List sub-agents".to_owned(),
        (_, Some(prompt)) => prompt.to_owned(),
        _ => "Sub-agent".to_owned(),
    }
}

/// What a finished item printed or returned.
fn output(item: &Value) -> Option<String> {
    let text = match item.get("type").and_then(Value::as_str)? {
        "commandExecution" => item.get("aggregatedOutput")?.as_str()?.to_owned(),
        "mcpToolCall" => {
            if let Some(error) = item.pointer("/error/message").and_then(Value::as_str) {
                error.to_owned()
            } else {
                item.pointer("/result/content")?
                    .as_array()?
                    .iter()
                    .filter_map(|block| block.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        _ => return None,
    };
    (!text.trim().is_empty()).then_some(text)
}

/// A file change's paths and unified diffs.
fn changes(item: &Value) -> Vec<(String, String)> {
    item.get("changes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|change| {
            let path = change.get("path")?.as_str()?.to_owned();
            let diff = change
                .get("diff")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            Some((path, diff))
        })
        .collect()
}

fn paths(item: &Value) -> Vec<String> {
    match item.get("type").and_then(Value::as_str) {
        Some("fileChange") => changes(item).into_iter().map(|(path, _)| path).collect(),
        Some("imageView") => item
            .get("path")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// Codex's plan as ACP's.
pub(crate) fn plan(params: &Value) -> wire::Plan {
    let entries = params
        .get("plan")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|step| {
            let status = match step.get("status").and_then(Value::as_str) {
                Some("inProgress") => wire::PlanEntryStatus::InProgress,
                Some("completed") => wire::PlanEntryStatus::Completed,
                _ => wire::PlanEntryStatus::Pending,
            };
            Some(wire::PlanEntry::new(
                step.get("step")?.as_str()?,
                wire::PlanEntryPriority::Medium,
                status,
            ))
        })
        .collect();
    wire::Plan::new(entries)
}
