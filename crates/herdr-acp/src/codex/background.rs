//! Codex's sub-agents and background terminals.
//!
//! A `collabAgentToolCall` item whose tool is `spawnAgent` starts a
//! sub-agent: a thread of its own whose events arrive on the same
//! connection under its thread id. Its calls show under the spawn call, and
//! it ends when its turn completes, when a later collab item reports its
//! state, or with a `subAgentActivity` item (multi-agent v2).
//!
//! Codex never says a command was sent to the background: a
//! `commandExecution` item that is still running when its turn completes
//! is one, and it ends with its `item/completed`, however much later. Its
//! activity starts when the turn ends, so its time is counted from there.

use std::collections::{HashMap, HashSet};

use herdr_chat_protocol::{ActivityKind, ActivityState};
use serde_json::Value;

use crate::project::{Report, Tag};

/// The most sub-agents, threads, and commands followed at once.
const MAX_TRACKED: usize = 256;

/// A report about a call other than the item that carried it.
pub(super) type Elsewhere = (String, Report);

#[derive(Debug, Default)]
pub(super) struct Background {
    /// Spawn calls whose sub-agent runs.
    agents: HashSet<String>,
    /// The spawn call of each sub-agent thread.
    children: HashMap<String, String>,
    /// Commands started and not yet completed, with their command line.
    commands: HashMap<String, String>,
    /// Commands that outlived their turn.
    shells: HashSet<String>,
}

impl Background {
    /// The tag a started item's call carries.
    pub(super) fn started(&mut self, item: &Value) -> Tag {
        let field = |key: &str| item.get(key).and_then(Value::as_str);
        let Some(id) = field("id") else {
            return Tag::default();
        };
        match field("type") {
            Some("collabAgentToolCall")
                if field("tool") == Some("spawnAgent") && self.agents.len() < MAX_TRACKED =>
            {
                self.agents.insert(id.to_owned());
                Tag::activity(Report::new(ActivityKind::Subagent, ActivityState::Running))
            }
            Some("commandExecution") if self.commands.len() < MAX_TRACKED => {
                self.commands.insert(
                    id.to_owned(),
                    field("command").unwrap_or_default().to_owned(),
                );
                Tag::default()
            }
            _ => Tag::default(),
        }
    }

    /// What a completed item says: about its own call's activity, and about
    /// sub-agents it reports on.
    pub(super) fn completed(&mut self, item: &Value) -> (Option<Report>, Vec<Elsewhere>) {
        let field = |key: &str| item.get(key).and_then(Value::as_str);
        let Some(id) = field("id") else {
            return (None, Vec::new());
        };
        let failed = matches!(field("status"), Some("failed" | "declined"));
        match field("type") {
            Some("collabAgentToolCall") => {
                let mut own = None;
                if self.agents.contains(id) {
                    if failed {
                        self.agents.remove(id);
                        own = Some(Report::new(ActivityKind::Subagent, ActivityState::Failed));
                    } else {
                        for thread in strings(item.get("receiverThreadIds")) {
                            if self.children.len() < MAX_TRACKED {
                                self.children.insert(thread.to_owned(), id.to_owned());
                            }
                        }
                    }
                }
                let mut others = Vec::new();
                for (thread, reported) in item
                    .get("agentsStates")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                {
                    let text = |key: &str| reported.get(key).and_then(Value::as_str);
                    let Some(state) = text("status").and_then(agent) else {
                        continue;
                    };
                    let detail = text("message")
                        .filter(|message| !message.is_empty())
                        .map(str::to_owned);
                    if let Some((call, mut report)) = self.end_child(thread, state) {
                        report.detail = detail;
                        if call == id {
                            own = Some(report);
                        } else {
                            others.push((call, report));
                        }
                    }
                }
                (own, others)
            }
            Some("subAgentActivity") => {
                let state = match field("kind") {
                    Some("completed") => ActivityState::Finished,
                    Some("interrupted") => ActivityState::Killed,
                    _ => return (None, Vec::new()),
                };
                let ended = field("agentThreadId").and_then(|thread| self.end_child(thread, state));
                (None, ended.into_iter().collect())
            }
            Some("commandExecution") => {
                self.commands.remove(id);
                if !self.shells.remove(id) {
                    return (None, Vec::new());
                }
                let state = match field("status") {
                    Some("failed") => ActivityState::Failed,
                    Some("declined" | "interrupted") => ActivityState::Killed,
                    _ => ActivityState::Finished,
                };
                let mut report = Report::new(ActivityKind::Shell, state);
                report.detail = item
                    .get("exitCode")
                    .and_then(Value::as_i64)
                    .map(|code| format!("Exited {code}"));
                (Some(report), Vec::new())
            }
            _ => (None, Vec::new()),
        }
    }

    /// The spawn call of the sub-agent thread `thread`.
    pub(super) fn parent(&self, thread: &str) -> Option<&str> {
        self.children.get(thread).map(String::as_str)
    }

    /// The sub-agent thread `thread` finished its turn.
    pub(super) fn child_turn_completed(
        &mut self,
        thread: &str,
        status: Option<&str>,
    ) -> Option<Elsewhere> {
        let state = match status {
            Some("failed") => ActivityState::Failed,
            Some("interrupted") => ActivityState::Killed,
            _ => ActivityState::Finished,
        };
        self.end_child(thread, state)
    }

    /// The turn ended: every command still running runs in the background.
    pub(super) fn turn_ended(&mut self) -> Vec<Elsewhere> {
        let mut started = Vec::new();
        for (id, command) in &self.commands {
            if self.shells.contains(id) {
                continue;
            }
            started.push((
                id.clone(),
                Report {
                    label: (!command.is_empty()).then(|| command.clone()),
                    ..Report::new(ActivityKind::Shell, ActivityState::Running)
                },
            ));
        }
        self.shells.extend(started.iter().map(|(id, _)| id.clone()));
        started
    }

    fn end_child(&mut self, thread: &str, state: ActivityState) -> Option<Elsewhere> {
        let call = self.children.remove(thread)?;
        if !self.agents.remove(&call) {
            return None;
        }
        Some((call, Report::new(ActivityKind::Subagent, state)))
    }
}

/// How Codex names a sub-agent's state once it ended; `None` while it runs.
fn agent(status: &str) -> Option<ActivityState> {
    match status {
        "completed" => Some(ActivityState::Finished),
        "errored" => Some(ActivityState::Failed),
        "interrupted" | "shutdown" | "notFound" => Some(ActivityState::Killed),
        _ => None,
    }
}

fn strings(value: Option<&Value>) -> impl Iterator<Item = &str> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

#[cfg(test)]
mod tests;
