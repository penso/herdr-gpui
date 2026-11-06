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

/// Whether a call keeps the to-do list, and so can go without a card. A call
/// hidden by mistake is one the user never sees, and a title is free text:
/// Copilot's comes from a `description` the model writes. So a title counts
/// only for an agent that sends no `name`, and only beside a list.
pub(super) fn is_tool(
    name: Option<&str>,
    title: &str,
    kind: wire::ToolKind,
    input: Option<&Value>,
) -> bool {
    let known = |shown: &str| TOOLS.iter().any(|tool| shown.eq_ignore_ascii_case(tool));
    let listed = input.is_some_and(|input| input.get("todos").is_some());
    list_kind(kind)
        && match name {
            Some(name) => known(name) && (listed || unwritten(input)),
            None => known(title) && listed,
        }
}

/// Whether an update leaves a hidden call a to-do call. One that says it does
/// something else goes back to showing as a card.
pub(super) fn stays_tool(kind: Option<wire::ToolKind>, input: Option<&Value>) -> bool {
    kind.is_none_or(list_kind)
        && (unwritten(input) || input.is_some_and(|input| input.get("todos").is_some()))
}

/// The kinds OpenCode and Copilot give their to-do tools, which neither run
/// nor change anything.
fn list_kind(kind: wire::ToolKind) -> bool {
    matches!(kind, wire::ToolKind::Other | wire::ToolKind::Think)
}

/// Whether a call's input has yet to come, as OpenCode's has not at first.
fn unwritten(input: Option<&Value>) -> bool {
    input.is_none_or(|input| input.as_object().is_some_and(serde_json::Map::is_empty))
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
