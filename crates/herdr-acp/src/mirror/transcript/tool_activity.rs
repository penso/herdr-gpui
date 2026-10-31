//! Last observed invocation state, not a guess about the agent's activity.
//! A returned spawn/wait call says nothing about a child task's completion.

use std::collections::BTreeMap;

use herdr_chat_protocol::{Entry, EntryKind, ToolStatus};
use serde_json::Value;

use super::{MAX_ENTRIES, tool_metadata};

#[derive(Clone, Copy)]
enum Operation {
    Command,
    Script,
    Other,
}

#[derive(Clone, Eq, PartialEq)]
enum Job {
    Command(String),
    Script(String),
}

#[derive(Clone, Eq, PartialEq)]
struct Run {
    job: Job,
    generation: usize,
}

struct Call {
    finished: bool,
    // Retain terminal generations too: a later poll must never fall back to
    // an older still-unresolved launch after a reused ID has completed.
    observed: Option<Run>,
    operation: Operation,
    resumes: Option<Run>,
}

#[derive(Default)]
pub(super) struct Activity {
    calls: BTreeMap<usize, Call>,
}

impl Activity {
    pub(super) fn record(&mut self, index: usize, name: &str, input: &Value) {
        let input = tool_metadata::arguments(input);
        let (operation, resumes) = match tool_metadata::name(name) {
            "Bash" | "exec_command" | "shell_command" => (Operation::Command, None),
            "write_stdin" => (
                Operation::Command,
                tool_metadata::identifier(input.get("session_id")).map(Job::Command),
            ),
            "exec" => (Operation::Script, None),
            "wait" => (
                Operation::Script,
                tool_metadata::identifier(input.get("cell_id")).map(Job::Script),
            ),
            _ => (Operation::Other, None),
        };
        // A poll belongs to the newest observed launch at the moment it was
        // submitted. Reused process/cell IDs cannot retarget an old poll.
        let resumes = resumes.and_then(|job| {
            self.calls
                .values()
                .filter_map(|call| call.observed.as_ref())
                .filter(|running| running.job == job)
                .max_by_key(|running| running.generation)
                .cloned()
        });
        self.calls.insert(
            index,
            Call {
                finished: false,
                observed: None,
                operation,
                resumes,
            },
        );
        if self.calls.len() > MAX_ENTRIES {
            self.calls.pop_first();
        }
    }

    pub(super) fn output(
        &mut self,
        index: usize,
        output: &Value,
        raw: &str,
        failed: bool,
        entries: &mut [Entry],
    ) {
        let operation = self
            .calls
            .get(&index)
            .map_or(Operation::Other, |call| call.operation);
        let result = if failed {
            Outcome::Failed
        } else {
            outcome(operation, output, raw)
        };
        let Some(call) = self.calls.get_mut(&index) else {
            set_status(entries, index, result.status());
            return;
        };
        // A replayed final or late intermediate chunk cannot reopen a known
        // terminal invocation or affect a new generation with the same ID.
        if call.finished {
            return;
        }
        if matches!(result, Outcome::Returned) && call.observed.is_some() {
            return;
        }
        set_status(entries, index, result.status());
        match result {
            Outcome::Running(job) => {
                let run = call
                    .observed
                    .as_ref()
                    .or(call.resumes.as_ref())
                    .filter(|run| run.job == job)
                    .cloned()
                    .unwrap_or(Run {
                        job,
                        generation: index,
                    });
                call.observed = Some(run);
            }
            Outcome::Finished(status) => {
                // Only a terminal result for the exact polled job updates earlier
                // cards. A failed polling request does not mean the job failed.
                let run = call.observed.clone().or_else(|| call.resumes.clone());
                call.finished = true;
                if let Some(run) = run {
                    for (previous, call) in &mut self.calls {
                        if !call.finished && call.observed.as_ref() == Some(&run) {
                            set_status(entries, *previous, status);
                            call.finished = true;
                        }
                    }
                }
            }
            Outcome::Failed => {
                call.finished = true;
            }
            Outcome::Returned => {}
        }
    }
}

fn set_status(entries: &mut [Entry], index: usize, status: ToolStatus) {
    if let Some(Entry {
        kind: EntryKind::Tool(tool),
        ..
    }) = entries.get_mut(index)
    {
        tool.status = status;
    }
}

enum Outcome {
    Returned,
    Failed,
    Running(Job),
    Finished(ToolStatus),
}

impl Outcome {
    fn status(&self) -> ToolStatus {
        match self {
            Self::Returned => ToolStatus::Completed,
            Self::Failed => ToolStatus::Failed,
            Self::Running(_) => ToolStatus::InProgress,
            Self::Finished(status) => *status,
        }
    }
}

fn outcome(operation: Operation, output: &Value, raw: &str) -> Outcome {
    let decoded = tool_metadata::arguments(output);
    // MCP's explicit error flag is evidence; the word "error" in ordinary
    // program output is not. Unknown result schemas otherwise stay opaque.
    if decoded.get("isError").and_then(Value::as_bool) == Some(true)
        && decoded.get("content").is_some_and(Value::is_array)
    {
        return Outcome::Failed;
    }
    if matches!(operation, Operation::Command) {
        if let Some(code) = decoded.get("exit_code").and_then(Value::as_i64) {
            return finished(code);
        }
        if decoded.get("output").is_some_and(Value::is_string)
            && let Some(id) = tool_metadata::identifier(decoded.get("session_id"))
        {
            return Outcome::Running(Job::Command(id));
        }
    }
    match operation {
        Operation::Command => command_wrapper(raw).unwrap_or(Outcome::Returned),
        Operation::Script => script_wrapper(raw).unwrap_or(Outcome::Returned),
        Operation::Other => Outcome::Returned,
    }
}

fn finished(code: i64) -> Outcome {
    Outcome::Finished(if code == 0 {
        ToolStatus::Completed
    } else {
        ToolStatus::Failed
    })
}

/// Inspect only the wrapper's header; program output can contain arbitrary
/// lookalike status lines. Unknown/new wrappers remain unclassified.
fn command_wrapper(raw: &str) -> Option<Outcome> {
    let (header, _) = raw
        .split_once("\nFinal output:")
        .or_else(|| raw.split_once("\nOutput:"))?;
    if header.len() > 1024 {
        return None;
    }
    let mut lines = header.lines();
    let first = lines.next()?;
    let wall_time = if first.starts_with("Chunk ID: ") {
        lines.next()?
    } else {
        first
    };
    if !valid_wall_time(wall_time) {
        return None;
    }
    let status = lines.next()?;
    if !valid_header_tail(lines) {
        return None;
    }
    if let Some(code) = status
        .strip_prefix("Process exited with code ")
        .or_else(|| status.strip_prefix("Exit code: "))
    {
        return code.parse::<i64>().ok().map(finished);
    }
    let id = status.strip_prefix("Process running with session ID ")?;
    id.parse::<u64>()
        .ok()
        .map(|id| Outcome::Running(Job::Command(id.to_string())))
}

fn script_wrapper(raw: &str) -> Option<Outcome> {
    let (header, _) = raw.split_once("\nOutput:")?;
    if header.len() > 1024 {
        return None;
    }
    let mut lines = header.lines();
    let status = lines.next()?;
    let wall_time = lines.next()?;
    if !valid_wall_time(wall_time) || !valid_header_tail(lines) {
        return None;
    }
    if status == "Script completed" {
        return Some(Outcome::Finished(ToolStatus::Completed));
    }
    if status == "Script failed" {
        return Some(Outcome::Finished(ToolStatus::Failed));
    }
    let id = status.strip_prefix("Script running with cell ID ")?;
    // Only an exact, bounded cell ID can later correlate a wait result.
    tool_metadata::identifier(Some(&Value::String(id.into())))
        .map(|id| Outcome::Running(Job::Script(id)))
}

fn valid_wall_time(line: &str) -> bool {
    line.strip_prefix("Wall time: ")
        .and_then(|time| time.strip_suffix(" seconds"))
        .and_then(|time| time.parse::<f64>().ok())
        .is_some_and(|time| time.is_finite() && time >= 0.0)
}

fn valid_header_tail(mut lines: std::str::Lines<'_>) -> bool {
    match lines.next() {
        None => true,
        Some(line) => {
            line.strip_prefix("Original token count: ")
                .and_then(|count| count.parse::<u64>().ok())
                .is_some()
                && lines.next().is_none()
        }
    }
}
