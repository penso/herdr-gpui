//! Claude's reported activity, not command stdout. Field names follow the
//! Agent SDK's SDKToolProgressMessage and SDKTask*Message wire types.

use super::*;
use crate::harness::streaming::Streams;

pub(super) const MAX_CALLS: usize = 2000;
const MAX_TASKS: usize = 2000;
const ID_BYTES: usize = 256;
const TEXT_BYTES: usize = 4096;

#[derive(Debug, Default)]
pub(super) struct Progress {
    pub(super) active: Streams<Activity>,
    // Completed runs remain as bounded tombstones. At capacity, new task IDs
    // are ignored rather than evicting the identity fence for late events.
    tasks: HashMap<String, Task>,
}

#[derive(Debug)]
struct Task {
    notified: bool,
    origin: String,
    row: String,
    run: Option<String>,
    terminal: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Activity {
    content: Option<String>,
    title: String,
}

impl Activity {
    fn render(&self, session: &str, id: &str) -> Out {
        out::update(
            session,
            wire::SessionUpdate::ToolCallUpdate(wire::ToolCallUpdate::new(
                id.to_owned(),
                wire::ToolCallUpdateFields::new()
                    .status(Some(wire::ToolCallStatus::InProgress))
                    .title(Some(self.title.clone()))
                    .content(self.content.as_deref().map(|text| vec![out::text(text)])),
            )),
        )
    }
}

impl Progress {
    pub(super) fn flush(&mut self, session: &str) -> Vec<Out> {
        self.active.flush(|id, value| value.render(session, id))
    }

    fn publish(&mut self, session: &str, id: &str, value: Activity) -> Vec<Out> {
        self.active
            .update(
                id,
                Instant::now(),
                |previous| {
                    if *previous == value {
                        return false;
                    }
                    *previous = value;
                    true
                },
                |id, value| value.render(session, id),
            )
            .into_iter()
            .collect()
    }
}

impl Bridge {
    pub(super) fn tool_progress(&mut self, message: &Value) -> Vec<Out> {
        if self.prompt.is_none() || !self.progress_session(message) {
            return Vec::new();
        }
        let Some(id) = identifier(message, "tool_use_id") else {
            return Vec::new();
        };
        let Some(call) = self
            .calls
            .get_mut(id)
            .filter(|call| !call.completed && visible(call))
        else {
            return Vec::new();
        };
        if message.get("tool_name").and_then(Value::as_str) != Some(call.name.as_str()) {
            return Vec::new();
        }
        let Some(elapsed) = message
            .get("elapsed_time_seconds")
            .and_then(Value::as_f64)
            .filter(|seconds| {
                seconds.is_finite() && *seconds >= 0.0 && *seconds <= u32::MAX as f64
            })
        else {
            return Vec::new();
        };
        let elapsed = elapsed.floor() as u32;
        if call.elapsed.is_some_and(|previous| previous >= elapsed) {
            return Vec::new();
        }
        call.elapsed = Some(elapsed);
        // A title update keeps a pending command's full input or edit diff.
        // The eventual tool_result restores the original title and output.
        let value = Activity {
            title: format!("{} · Running for {elapsed}s", call.title),
            content: None,
        };
        if !self.progress.active.contains(id) {
            self.progress.active.insert(id, Activity::default());
        }
        self.progress.publish(&self.session, id, value)
    }

    pub(super) fn task_event(&mut self, message: &Value) -> Vec<Out> {
        if !self.progress_session(message) {
            return Vec::new();
        }
        let Some(id) = identifier(message, "task_id") else {
            return Vec::new();
        };
        let run = match message.get("run_id") {
            None => None,
            Some(_) => match identifier(message, "run_id") {
                Some(run) => Some(run),
                None => return Vec::new(),
            },
        };
        let kind = message.get("subtype").and_then(Value::as_str);
        if kind == Some("task_started") {
            let mut outs = self.task_started(message, id, run);
            // An SDK event can lack the description needed for a progress
            // card while still naming a known background activity. A rejected
            // replay must not retag a task whose run we already know.
            if !outs.is_empty() || !self.progress.tasks.contains_key(id) {
                outs.extend(self.system(message));
            }
            return outs;
        }
        let Some(task) = self.progress.tasks.get(id) else {
            return self.system(message);
        };
        // Run IDs are opaque but ordered by the SDK. Events without a run ID
        // cannot update a run whose identity was explicitly reported.
        if task.run.as_deref() != run
            || message
                .get("tool_use_id")
                .is_some_and(|origin| origin.as_str() != Some(task.origin.as_str()))
        {
            return Vec::new();
        }
        let row = task.row.clone();
        match kind {
            Some("task_progress") if !task.terminal => {
                let Some(description) = message.get("description").and_then(Value::as_str) else {
                    return Vec::new();
                };
                self.progress.publish(
                    &self.session,
                    &row,
                    Activity {
                        title: format!("Task: {}", text(description)),
                        content: Some(task_content(message)),
                    },
                )
            }
            Some("task_notification") if !task.notified => {
                let Some(status) = terminal(message.get("status").and_then(Value::as_str)) else {
                    return Vec::new();
                };
                let mut outs = self.finish_task(id, status, Some(task_content(message)), true);
                outs.extend(self.system(message));
                outs
            }
            Some("task_updated") if !task.terminal => {
                let patch = &message["patch"];
                let Some(status) = terminal(patch.get("status").and_then(Value::as_str)) else {
                    return Vec::new();
                };
                let content = patch.get("error").and_then(Value::as_str).map(text);
                self.finish_task(id, status, content, false)
            }
            _ => Vec::new(),
        }
    }

    fn progress_session(&self, message: &Value) -> bool {
        message.get("session_id").and_then(Value::as_str) == Some(self.session.as_str())
            && message.get("parent_tool_use_id").is_none_or(Value::is_null)
            && message.get("parent_task_id").is_none_or(Value::is_null)
            && message
                .get("spawn_depth")
                .is_none_or(|depth| depth.as_u64().is_some_and(|depth| depth <= 1))
            && message
                .get("skip_transcript")
                .is_none_or(|value| value.as_bool() == Some(false))
            && message
                .get("ambient")
                .is_none_or(|value| value.as_bool() == Some(false))
    }

    fn task_started(&mut self, message: &Value, id: &str, run: Option<&str>) -> Vec<Out> {
        let Some(origin) = identifier(message, "tool_use_id") else {
            return Vec::new();
        };
        let Some(call) = self.calls.get(origin).filter(|call| visible(call)) else {
            return Vec::new();
        };
        let Some(description) = message.get("description").and_then(Value::as_str) else {
            return Vec::new();
        };
        if let Some(previous) = self.progress.tasks.get(id) {
            // A task can resume under its existing ID only after completion,
            // and only with a strictly newer, explicitly reported run ID.
            if !previous.terminal || run.is_none() || run <= previous.run.as_deref() {
                return Vec::new();
            }
        } else if self.progress.tasks.len() >= MAX_TASKS {
            return Vec::new();
        }
        let row = format!("claude-task:{}:{id}:{}", id.len(), run.unwrap_or_default());
        if !self.progress.active.insert(&row, Activity::default()) {
            return Vec::new();
        }
        self.progress.tasks.insert(
            id.to_owned(),
            Task {
                notified: false,
                origin: origin.to_owned(),
                row: row.clone(),
                run: run.map(str::to_owned),
                terminal: false,
            },
        );
        let kind = tools::kind(&call.name);
        vec![
            self.update(wire::SessionUpdate::ToolCall(
                wire::ToolCall::new(row, format!("Task: {}", text(description)))
                    .kind(kind)
                    .status(wire::ToolCallStatus::InProgress),
            )),
        ]
    }

    fn finish_task(
        &mut self,
        id: &str,
        (status, label): (wire::ToolCallStatus, &str),
        content: Option<String>,
        notified: bool,
    ) -> Vec<Out> {
        let Some(task) = self.progress.tasks.get_mut(id) else {
            return Vec::new();
        };
        task.terminal = true;
        task.notified |= notified;
        let row = task.row.clone();
        // Completion is authoritative: discard any queued partial publication.
        let previous = self.progress.active.remove(&row);
        let content = content.or_else(|| previous.and_then(|value| value.content));
        let text = content.map_or_else(|| label.to_owned(), |text| format!("{label}\n{text}"));
        vec![
            self.update(wire::SessionUpdate::ToolCallUpdate(
                wire::ToolCallUpdate::new(
                    row,
                    wire::ToolCallUpdateFields::new()
                        .status(Some(status))
                        .content(Some(vec![out::text(&text)])),
                ),
            )),
        ]
    }
}

fn visible(call: &Call) -> bool {
    !call.hidden && !matches!(call.name.as_str(), tools::QUESTION | tools::EXIT_PLAN)
}

pub(super) fn identifier<'a>(message: &'a Value, key: &str) -> Option<&'a str> {
    message.get(key)?.as_str().filter(|id| {
        !id.is_empty()
            && id.len() <= ID_BYTES
            && !id.chars().any(char::is_whitespace)
            && !id.chars().any(char::is_control)
    })
}

pub(super) fn text(value: &str) -> String {
    if value.len() <= TEXT_BYTES {
        return value.to_owned();
    }
    let end = value.floor_char_boundary(TEXT_BYTES - '…'.len_utf8());
    format!("{}…", &value[..end])
}

fn terminal(status: Option<&str>) -> Option<(wire::ToolCallStatus, &'static str)> {
    match status? {
        "completed" => Some((wire::ToolCallStatus::Completed, "Task completed")),
        "failed" => Some((wire::ToolCallStatus::Failed, "Task failed")),
        "stopped" | "killed" => Some((wire::ToolCallStatus::Failed, "Task stopped")),
        _ => None,
    }
}

fn task_content(message: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(summary) = message
        .get("summary")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        parts.push(format!("Reported progress: {}", text(summary)));
    }
    if let Some(tool) = message.get("last_tool_name").and_then(Value::as_str) {
        parts.push(format!("Latest tool: {}", text(tool)));
    }
    let usage = &message["usage"];
    if let Some(uses) = usage.get("tool_uses").and_then(Value::as_u64) {
        parts.push(format!("Tools used: {uses}"));
    }
    if let Some(duration) = usage.get("duration_ms").and_then(Value::as_u64) {
        parts.push(format!("Elapsed: {}s", duration / 1000));
    }
    if let Some(tokens) = usage.get("total_tokens").and_then(Value::as_u64) {
        parts.push(format!("Tokens: {tokens}"));
    }
    // output_file and nested task messages are deliberately not interpreted.
    text(&parts.join("\n"))
}
