//! Claude Code's task list as the plan. Where `TodoWrite` sends the whole
//! list, these tools change one task at a time and an update usually names
//! only an id and a status, so the list is kept here. A task counts once its
//! call has succeeded, and the id of a new one comes from that result.
//!
//! The list starts empty when a session is resumed; Claude's `TaskList` fills
//! it in. A sub-agent's calls are not followed, as with its to-do list.

use acp::v1 as wire;
use serde_json::Value;

use crate::project::todos::{self, MAX_PLAN};

const CREATE: &str = "TaskCreate";
const GET: &str = "TaskGet";
const LIST: &str = "TaskList";
const UPDATE: &str = "TaskUpdate";

/// Whether the tool `name` works on the task list, and so shows as the plan
/// rather than as a call. `Task` and `TaskStop` are about sub-agents and
/// background shells instead.
pub(super) fn is_tool(name: &str) -> bool {
    matches!(name, CREATE | GET | LIST | UPDATE)
}

#[derive(Clone, Debug, PartialEq)]
struct Task {
    id: String,
    status: wire::PlanEntryStatus,
    subject: String,
}

#[derive(Debug, Default)]
pub(super) struct Tasks {
    list: Vec<Task>,
}

impl Tasks {
    /// Takes in a call of the tool `name` that succeeded, given its input,
    /// its output, and Claude Code's structured result if it came with one.
    /// The plan as it now stands, when the call changed it.
    pub(super) fn finished(
        &mut self,
        name: &str,
        input: &Value,
        output: &Value,
        structured: Option<&Value>,
    ) -> Option<wire::Plan> {
        let before = self.list.clone();
        match name {
            CREATE => self.created(input, output, structured),
            LIST => self.listed(structured),
            UPDATE => self.updated(input, structured),
            _ => {}
        }
        (self.list != before).then(|| self.plan())
    }

    fn created(&mut self, input: &Value, output: &Value, structured: Option<&Value>) {
        let id = structured
            .and_then(|result| result.pointer("/task/id"))
            .and_then(Value::as_str)
            .or_else(|| created_id(output));
        let subject = input.get("subject").and_then(Value::as_str);
        let (Some(id), Some(subject)) = (id, subject) else {
            return;
        };
        if self.list.len() < MAX_PLAN && !self.list.iter().any(|task| task.id == id) {
            self.list.push(Task {
                id: id.to_owned(),
                status: wire::PlanEntryStatus::Pending,
                subject: subject.to_owned(),
            });
        }
    }

    fn updated(&mut self, input: &Value, structured: Option<&Value>) {
        let refused = structured
            .and_then(|result| result.get("success"))
            .and_then(Value::as_bool)
            == Some(false);
        let field = |key: &str| input.get(key).and_then(Value::as_str);
        let Some(id) = field("taskId").filter(|_| !refused) else {
            return;
        };
        if field("status") == Some("deleted") {
            self.list.retain(|task| task.id != id);
            return;
        }
        let status = field("status").and_then(todos::status);
        let subject = field("subject");
        if let Some(task) = self.list.iter_mut().find(|task| task.id == id) {
            if let Some(status) = status {
                task.status = status;
            }
            if let Some(subject) = subject {
                subject.clone_into(&mut task.subject);
            }
        } else if let Some(subject) = subject.filter(|_| self.list.len() < MAX_PLAN) {
            // One made before this list began. Without a subject there is
            // nothing to show for it until Claude lists its tasks.
            self.list.push(Task {
                id: id.to_owned(),
                status: status.unwrap_or(wire::PlanEntryStatus::Pending),
                subject: subject.to_owned(),
            });
        }
    }

    /// Claude's own list replaces this one.
    fn listed(&mut self, structured: Option<&Value>) {
        let Some(tasks) = structured
            .and_then(|result| result.get("tasks"))
            .and_then(Value::as_array)
        else {
            return;
        };
        self.list = tasks
            .iter()
            .filter_map(|task| {
                let field = |key: &str| task.get(key).and_then(Value::as_str);
                Some(Task {
                    id: field("id")?.to_owned(),
                    status: todos::status(field("status")?)?,
                    subject: field("subject")?.to_owned(),
                })
            })
            .take(MAX_PLAN)
            .collect();
    }

    fn plan(&self) -> wire::Plan {
        wire::Plan::new(
            self.list
                .iter()
                .map(|task| todos::step(&task.subject, task.status.clone()))
                .collect(),
        )
    }
}

/// The id in `Task #1 created successfully: Fix it`, for a result that came
/// without its structured form, as one of several in a message does.
fn created_id(output: &Value) -> Option<&str> {
    let text = output.as_str().or_else(|| {
        output
            .as_array()?
            .iter()
            .find_map(|block| block.get("text")?.as_str())
    })?;
    let (id, rest) = text.strip_prefix("Task #")?.split_once(' ')?;
    rest.starts_with("created successfully").then_some(id)
}
