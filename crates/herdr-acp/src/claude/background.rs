//! Claude Code's sub-agents and background shells: which call started each,
//! the task id Claude Code names it by afterwards, and how it ended.
//!
//! A call to `Agent` (formerly `Task`) starts a sub-agent; a `Bash` call with
//! `run_in_background`, or one Claude Code moved to the background, starts a
//! shell. Either ends with its result when it ran in the foreground, or
//! later with a task notification (a `task_notification` system message or
//! a `<task-notification>` prompt), a `TaskStop` call, or a `TaskOutput`
//! result that reports an exit.

use std::collections::HashMap;

use herdr_chat_protocol::{ActivityKind, ActivityState};
use serde_json::Value;

use crate::project::{Report, Tag};

/// The most activities followed at once; a call past it stays a card.
const MAX_TRACKED: usize = 256;

#[derive(Debug)]
struct Track {
    /// Its result came and it runs on: Claude Code gave it a task id.
    detached: bool,
    kind: ActivityKind,
}

#[derive(Debug, Default)]
pub(crate) struct Background {
    /// Calls whose activity still runs, by call id.
    running: HashMap<String, Track>,
    /// The call each task id belongs to.
    tasks: HashMap<String, String>,
    /// `TaskStop` and `TaskOutput` calls waiting for their result, with the
    /// task each is about.
    waiting: HashMap<String, String>,
}

/// A report about a call other than the one whose message carried it.
pub(crate) type Elsewhere = (String, Report);

impl Background {
    /// The tag a new call starts with, for a sub-agent's step when `parent`
    /// names the sub-agent.
    pub(crate) fn started(
        &mut self,
        call: &str,
        name: &str,
        input: &Value,
        parent: Option<&str>,
    ) -> Tag {
        let field = |key: &str| input.get(key).and_then(Value::as_str);
        let kind = match name {
            "Agent" | "Task" => Some(ActivityKind::Subagent),
            "Bash" if input.get("run_in_background").and_then(Value::as_bool) == Some(true) => {
                Some(ActivityKind::Shell)
            }
            _ => None,
        };
        if matches!(
            name,
            "TaskStop" | "KillShell" | "KillBash" | "TaskOutput" | "BashOutput"
        ) && let Some(task) = field("task_id")
            .or_else(|| field("shell_id"))
            .or_else(|| field("bash_id"))
            && self.waiting.len() < MAX_TRACKED
        {
            self.waiting.insert(call.to_owned(), task.to_owned());
        }
        let activity = kind
            .filter(|kind| self.track(call, *kind))
            .map(|kind| Report {
                label: field("command")
                    .filter(|_| kind == ActivityKind::Shell)
                    .map(str::to_owned),
                ..Report::new(kind, ActivityState::Running)
            });
        Tag {
            activity,
            parent: parent.map(str::to_owned),
        }
    }

    /// What the result of `call` (a call to `name`) says: about its own
    /// activity, and about the activity of the call it stopped or polled.
    pub(crate) fn result(
        &mut self,
        call: &str,
        name: &str,
        failed: bool,
        output: &Value,
        structured: Option<&Value>,
    ) -> (Option<Report>, Option<Elsewhere>) {
        let text = text_of(output);
        let field = |key: &str| {
            structured
                .and_then(|result| result.get(key))
                .and_then(Value::as_str)
        };
        if let Some(task) = self.waiting.remove(call) {
            let state = match name {
                _ if failed => None,
                "TaskStop" | "KillShell" | "KillBash" => Some(ActivityState::Killed),
                _ => element(&text, "status").and_then(state),
            };
            let detail = element(&text, "exit_code").map(|code| format!("Exited {code}"));
            let other = state.and_then(|state| self.end(&task, state, detail));
            return (None, other);
        }
        match name {
            "Bash" => {
                let task = field("backgroundTaskId")
                    .map(str::to_owned)
                    .or_else(|| id_after(&text, "Command running in background with ID: "));
                if let Some(task) = task.filter(|_| !failed) {
                    // A command that ran too long is moved to the background
                    // without asking for it.
                    let known = self.running.contains_key(call);
                    if !known && !self.track(call, ActivityKind::Shell) {
                        return (None, None);
                    }
                    self.detach(call, task);
                    return (
                        (!known).then(|| Report::new(ActivityKind::Shell, ActivityState::Running)),
                        None,
                    );
                }
                (self.finish(call, failed), None)
            }
            "Agent" | "Task" if self.running.contains_key(call) => {
                let launched = structured.is_some_and(|result| {
                    result.get("isAsync").and_then(Value::as_bool) == Some(true)
                        || result.get("status").and_then(Value::as_str) == Some("async_launched")
                }) || text.starts_with("Async agent launched");
                let agent = field("agentId")
                    .map(str::to_owned)
                    .or_else(|| id_after(&text, "agentId: "));
                match agent.filter(|_| launched && !failed) {
                    Some(agent) => {
                        self.detach(call, agent);
                        (None, None)
                    }
                    None => (self.finish(call, failed), None),
                }
            }
            _ => (None, None),
        }
    }

    /// Claude Code says a task started; the call it names is now known by
    /// its task id too.
    pub(crate) fn task_started(&mut self, task: &str, call: Option<&str>) {
        if let Some(call) = call.filter(|call| self.running.contains_key(*call)) {
            self.detach(call, task.to_owned());
        }
    }

    /// A task notification: the task `task`, or the call `call`, ended with
    /// `status`.
    pub(crate) fn notified(
        &mut self,
        task: &str,
        call: Option<&str>,
        status: &str,
        summary: Option<&str>,
    ) -> Option<Elsewhere> {
        let state = state(status)?;
        let call = call
            .filter(|call| self.running.contains_key(*call))
            .map(str::to_owned)
            .or_else(|| self.tasks.get(task).cloned())?;
        self.tasks.remove(task);
        let track = self.running.remove(&call)?;
        let mut report = Report::new(track.kind, state);
        report.detail = summary.map(str::to_owned);
        Some((call, report))
    }

    /// The turn ended: what still waited for its own result was stopped
    /// with it. Detached work runs on.
    pub(crate) fn turn_ended(&mut self) -> Vec<Elsewhere> {
        let stopped: Vec<String> = self
            .running
            .iter()
            .filter(|(_, track)| !track.detached)
            .map(|(call, _)| call.clone())
            .collect();
        self.waiting.clear();
        stopped
            .into_iter()
            .filter_map(|call| {
                let track = self.running.remove(&call)?;
                Some((call, Report::new(track.kind, ActivityState::Killed)))
            })
            .collect()
    }

    fn track(&mut self, call: &str, kind: ActivityKind) -> bool {
        if self.running.len() >= MAX_TRACKED {
            return false;
        }
        self.running.insert(
            call.to_owned(),
            Track {
                detached: false,
                kind,
            },
        );
        true
    }

    fn detach(&mut self, call: &str, task: String) {
        if let Some(track) = self.running.get_mut(call) {
            track.detached = true;
        }
        if self.tasks.len() >= MAX_TRACKED {
            let running = &self.running;
            self.tasks.retain(|_, call| running.contains_key(call));
        }
        if self.tasks.len() < MAX_TRACKED {
            self.tasks.insert(task, call.to_owned());
        }
    }

    /// The foreground call `call` ended with its result.
    fn finish(&mut self, call: &str, failed: bool) -> Option<Report> {
        let track = self.running.remove(call)?;
        let state = if failed {
            ActivityState::Failed
        } else {
            ActivityState::Finished
        };
        Some(Report::new(track.kind, state))
    }

    /// The task `task` ended with `state`, as another call found.
    fn end(
        &mut self,
        task: &str,
        state: ActivityState,
        detail: Option<String>,
    ) -> Option<Elsewhere> {
        let call = self.tasks.remove(task)?;
        let track = self.running.remove(&call)?;
        let mut report = Report::new(track.kind, state);
        report.detail = detail;
        Some((call, report))
    }
}

/// How Claude Code names an ended task's outcome; `None` while it runs.
fn state(status: &str) -> Option<ActivityState> {
    match status {
        "completed" => Some(ActivityState::Finished),
        "failed" => Some(ActivityState::Failed),
        "killed" | "stopped" => Some(ActivityState::Killed),
        _ => None,
    }
}

/// A notification in the `<task-notification>` envelope Claude Code sends
/// the model: the task id, the call when it names one, the status, and the
/// summary. Only these display fields are read; nothing in it is followed.
pub(crate) fn task_notification(text: &str) -> Option<(&str, Option<&str>, &str, Option<&str>)> {
    let inner = text
        .trim()
        .strip_prefix("<task-notification>")?
        .strip_suffix("</task-notification>")?;
    Some((
        element(inner, "task-id")?,
        element(inner, "tool-use-id"),
        element(inner, "status")?,
        element(inner, "summary"),
    ))
}

/// The text of the first `<name>…</name>` in `text`, when it holds no markup.
fn element<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let value = text.split_once(&open)?.1.split_once(&close)?.0.trim();
    (!value.contains(['<', '>'])).then_some(value)
}

/// The id right after `prefix` in `text`.
fn id_after(text: &str, prefix: &str) -> Option<String> {
    let rest = text.split_once(prefix)?.1;
    let id: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        .take(128)
        .collect();
    (!id.is_empty()).then_some(id)
}

/// A tool result's text, whether a string or text blocks.
fn text_of(output: &Value) -> String {
    match output {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests;
