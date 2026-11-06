//! To-do tools read as the plan. ACP has an update for the plan, but OpenCode
//! and Copilot send none: each keeps its list with a tool of its own, whose
//! call carries the whole list. This goes by the tool's name and input, which
//! neither promises to keep, so a `plan` update always wins over it.

use acp::v1 as wire;
use serde_json::Value;

/// The most steps of one plan kept; the rail lists far fewer.
pub(crate) const MAX_PLAN: usize = 256;

/// OpenCode's and Copilot's to-do tools.
const TOOLS: [&str; 2] = ["todowrite", "update_todo"];

/// Whether a call named `name` or titled `title` keeps the to-do list.
pub(super) fn is_tool(name: Option<&str>, title: Option<&str>) -> bool {
    [name, title]
        .into_iter()
        .flatten()
        .any(|shown| TOOLS.iter().any(|tool| shown.eq_ignore_ascii_case(tool)))
}

/// The list in a to-do tool's input: steps with a status, or a Markdown
/// checklist. `None` when the input holds no list, as OpenCode's does while
/// the call is still being written; an empty list is one that was cleared.
pub(crate) fn plan(input: &Value) -> Option<wire::Plan> {
    match input.get("todos")? {
        Value::Array(todos) => Some(listed(todos)),
        Value::String(text) => Some(checklist(text)),
        _ => None,
    }
}

/// The status a to-do tool names, when it is one the plan has.
pub(crate) fn status(name: &str) -> Option<wire::PlanEntryStatus> {
    match name {
        "pending" => Some(wire::PlanEntryStatus::Pending),
        "in_progress" => Some(wire::PlanEntryStatus::InProgress),
        "completed" => Some(wire::PlanEntryStatus::Completed),
        _ => None,
    }
}

/// A step of the plan, in the priority no to-do tool has.
pub(crate) fn step(content: &str, status: wire::PlanEntryStatus) -> wire::PlanEntry {
    wire::PlanEntry::new(content, wire::PlanEntryPriority::Medium, status)
}

/// ACP's plan has no cancelled step, and one no longer planned is not shown.
fn listed(todos: &[Value]) -> wire::Plan {
    let entries = todos
        .iter()
        .filter_map(|todo| {
            let name = todo.get("status").and_then(Value::as_str);
            if name == Some("cancelled") {
                return None;
            }
            let status = name.and_then(status);
            let content = todo.get("content")?.as_str()?;
            Some(step(
                content,
                status.unwrap_or(wire::PlanEntryStatus::Pending),
            ))
        })
        .take(MAX_PLAN)
        .collect();
    wire::Plan::new(entries)
}

/// A checklist says only what is done, so no step shows as in progress.
fn checklist(text: &str) -> wire::Plan {
    wire::Plan::new(text.lines().filter_map(item).take(MAX_PLAN).collect())
}

/// The step on a line such as `- [x] Read the code` or `2. [ ] Fix it`.
fn item(line: &str) -> Option<wire::PlanEntry> {
    let line = line.trim_start();
    let rest = match line.strip_prefix(['-', '*', '+']) {
        Some(rest) => rest,
        None => line
            .trim_start_matches(|digit: char| digit.is_ascii_digit())
            .strip_prefix(['.', ')'])?,
    };
    let rest = rest.trim_start().strip_prefix('[')?;
    let (mark, content) = rest.split_once(']')?;
    let status = match mark.trim() {
        "" => wire::PlanEntryStatus::Pending,
        "x" | "X" => wire::PlanEntryStatus::Completed,
        _ => return None,
    };
    let content = content.trim();
    (!content.is_empty()).then(|| step(content, status))
}
