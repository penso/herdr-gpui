//! Work the agent started that runs beside the conversation: sub-agents and
//! background shells, which can outlive the turn that started them.

use serde::{Deserialize, Serialize};

/// The most activities a transcript keeps; a new one replaces the oldest
/// that has ended.
pub const MAX_ACTIVITIES: usize = 32;
/// The longest call id, label, or detail an activity carries, in bytes.
pub const MAX_ACTIVITY_TEXT: usize = 1024;

/// One sub-agent or background shell, from the tool call that started it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    /// The tool call that started it, whose card shows it in the
    /// conversation; unique among activities.
    pub call_id: String,
    /// What it is doing now, such as a sub-agent's latest step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// When it ended (Unix time), once it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_ms: Option<u64>,
    #[serde(default)]
    pub kind: ActivityKind,
    /// What it is for: a sub-agent's description or a shell's command.
    pub label: String,
    /// When it started (Unix time).
    pub started_ms: u64,
    #[serde(default)]
    pub state: ActivityState,
}

impl Activity {
    pub fn is_running(&self) -> bool {
        self.state == ActivityState::Running
    }

    /// Whether every string fits [`MAX_ACTIVITY_TEXT`].
    pub fn fits(&self) -> bool {
        [Some(&self.call_id), Some(&self.label), self.detail.as_ref()]
            .into_iter()
            .flatten()
            .all(|text| text.len() <= MAX_ACTIVITY_TEXT)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    /// Another agent the agent delegated a task to.
    Subagent,
    /// A command left running in the background.
    Shell,
    /// A kind from a newer program.
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    #[default]
    Running,
    Failed,
    /// Stopped before it ended: by the agent, the user, or the agent exiting.
    Killed,
    /// Ended on its own, successfully or without saying; also an outcome
    /// from a newer program.
    #[serde(other)]
    Finished,
}

/// How many activities of each kind still run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Running {
    pub other: usize,
    pub shells: usize,
    pub subagents: usize,
}

impl Running {
    pub fn total(self) -> usize {
        self.other + self.shells + self.subagents
    }
}
