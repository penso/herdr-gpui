//! How Claude Code's tools read in the transcript: a title, a kind, what a
//! call shows before it runs, and what its result adds.

use acp::v1 as wire;
use serde_json::Value;

use crate::harness::out;

/// The tool that keeps Claude's to-do list, shown as the plan rather than
/// as a call.
pub(super) const TODOS: &str = "TodoWrite";
/// The tool that asks to leave plan mode, with the plan to approve.
pub(super) const EXIT_PLAN: &str = "ExitPlanMode";
/// The tool that asks the user a question with choices.
pub(super) const QUESTION: &str = "AskUserQuestion";

/// A call as it first appears.
pub(super) fn call(id: &str, name: &str, input: &Value) -> wire::ToolCall {
    let mut call = wire::ToolCall::new(id.to_owned(), title(name, input))
        .kind(kind(name))
        .status(wire::ToolCallStatus::Pending)
        .content(content(name, input))
        .raw_input(input.clone());
    if let Some(path) = path(input) {
        let mut location = wire::ToolCallLocation::new(path);
        if let Some(line) = input.get("offset").and_then(Value::as_u64) {
            location = location.line(u32::try_from(line).ok());
        }
        call = call.locations(vec![location]);
    }
    call
}

/// What a finished call adds: its output, or nothing for a call whose
/// content already says what it did (an edit's diff, a read's file).
pub(super) fn result(
    name: &str,
    output: &Value,
    failed: bool,
) -> Option<Vec<wire::ToolCallContent>> {
    let shows_output = failed || !matches!(kind(name), wire::ToolKind::Edit | wire::ToolKind::Read);
    shows_output.then(|| output_content(output))
}

pub(super) fn title(name: &str, input: &Value) -> String {
    let field = |key: &str| input.get(key).and_then(Value::as_str);
    let titled = match name {
        "Bash" => field("command").map(out::command_title),
        "Read" => field("file_path").map(|path| format!("Read {path}")),
        "Write" => field("file_path").map(|path| format!("Write {path}")),
        "Edit" | "MultiEdit" => field("file_path").map(|path| format!("Edit {path}")),
        "NotebookEdit" => field("notebook_path").map(|path| format!("Edit {path}")),
        "Glob" => field("pattern").map(|pattern| format!("Find {pattern}")),
        "Grep" => field("pattern").map(|pattern| format!("Search for {pattern}")),
        "WebFetch" => field("url").map(|url| format!("Fetch {url}")),
        "WebSearch" => field("query").map(|query| format!("Search the web for {query}")),
        "Task" | "Agent" => field("description").map(str::to_owned),
        EXIT_PLAN => Some("Ready to code?".to_owned()),
        QUESTION => question(input).map(str::to_owned),
        _ => None,
    };
    titled.unwrap_or_else(|| name.to_owned())
}

fn kind(name: &str) -> wire::ToolKind {
    match name {
        "Bash" | "BashOutput" | "KillShell" => wire::ToolKind::Execute,
        "Read" => wire::ToolKind::Read,
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => wire::ToolKind::Edit,
        "Glob" | "Grep" | "WebSearch" | "ToolSearch" => wire::ToolKind::Search,
        "WebFetch" => wire::ToolKind::Fetch,
        "Task" | "Agent" => wire::ToolKind::Think,
        EXIT_PLAN => wire::ToolKind::SwitchMode,
        _ => wire::ToolKind::Other,
    }
}

/// What a call shows before it runs: an edit's diff, the plan to approve.
fn content(name: &str, input: &Value) -> Vec<wire::ToolCallContent> {
    let field = |key: &str| input.get(key).and_then(Value::as_str);
    let diff = |path: &str, old: Option<&str>, new: &str| {
        wire::ToolCallContent::Diff(
            wire::Diff::new(path.to_owned(), new.to_owned()).old_text(old.map(str::to_owned)),
        )
    };
    match (name, field("file_path")) {
        ("Write", Some(path)) => vec![diff(path, None, field("content").unwrap_or_default())],
        ("Edit", Some(path)) => vec![diff(
            path,
            field("old_string"),
            field("new_string").unwrap_or_default(),
        )],
        ("MultiEdit", Some(path)) => input
            .get("edits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|edit| {
                let text = |key: &str| edit.get(key).and_then(Value::as_str);
                diff(
                    path,
                    text("old_string"),
                    text("new_string").unwrap_or_default(),
                )
            })
            .collect(),
        (EXIT_PLAN, _) => field("plan").map(out::text).into_iter().collect(),
        // The whole command, so what a permission request runs is never
        // hidden behind its one-line title.
        ("Bash", _) => field("command")
            .filter(|command| command.lines().nth(1).is_some())
            .map(out::text)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// A tool result's text and pictures.
fn output_content(output: &Value) -> Vec<wire::ToolCallContent> {
    match output {
        Value::String(output) if !output.is_empty() => vec![out::text(output)],
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| match block.get("type").and_then(Value::as_str) {
                Some("text") => block.get("text").and_then(Value::as_str).map(out::text),
                Some("image") => {
                    let source = block.get("source")?;
                    let data = source.get("data")?.as_str()?;
                    let mime = source.get("media_type")?.as_str()?;
                    Some(wire::ToolCallContent::Content(wire::Content::new(
                        wire::ContentBlock::Image(wire::ImageContent::new(data, mime)),
                    )))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn path(input: &Value) -> Option<&str> {
    ["file_path", "notebook_path", "path"]
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
}

/// The question of an `AskUserQuestion` call that asks exactly one.
pub(super) fn question(input: &Value) -> Option<&str> {
    match input.get("questions")?.as_array()?.as_slice() {
        [only] => only.get("question")?.as_str(),
        _ => None,
    }
}

/// Claude's to-do list as the plan.
pub(super) fn plan(input: &Value) -> wire::Plan {
    let entries = input
        .get("todos")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|todo| {
            let content = todo.get("content")?.as_str()?;
            let status = match todo.get("status").and_then(Value::as_str) {
                Some("in_progress") => wire::PlanEntryStatus::InProgress,
                Some("completed") => wire::PlanEntryStatus::Completed,
                _ => wire::PlanEntryStatus::Pending,
            };
            Some(wire::PlanEntry::new(
                content,
                wire::PlanEntryPriority::Medium,
                status,
            ))
        })
        .collect();
    wire::Plan::new(entries)
}

#[cfg(test)]
mod tests;
