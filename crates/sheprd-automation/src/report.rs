//! What `herdr-automate ... --json` prints: the contract between the
//! daemon's command line and its clients, Sheprd first. Plain types with
//! no engine behind them, so a client can read them without building Luau.
//! Times are Unix seconds.

use serde::{Deserialize, Serialize};

/// Bumped when a client could misread a report.
pub const PROTOCOL: u32 = 1;

/// The longest log text one answer carries.
pub const MAX_LOG_BYTES: usize = 256 * 1024;

/// `herdr-automate status --json`: everything a client shows for a host.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub protocol: u32,
    pub version: String,
    pub daemon: Daemon,
    pub caps: HostCaps,
    /// Tokens every automated agent used today, in the host's zone.
    pub today: Tokens,
    pub scripts: Vec<ScriptStatus>,
    /// Recent runs of every script, newest first, at most [`RECENT_RUNS`].
    pub runs: Vec<RunSummary>,
}

pub const RECENT_RUNS: usize = 50;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Daemon {
    /// A daemon holds the lock and beat within the last minute.
    pub running: bool,
    pub pid: Option<u32>,
    pub version: Option<String>,
    pub started_at: Option<i64>,
    pub beat_at: Option<i64>,
    /// Scheduling is paused on this host; running agents go on.
    pub paused_all: bool,
    /// A user service keeps the daemon running across reboots.
    pub service: bool,
}

/// Caps for the whole host, on top of each script's own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCaps {
    pub max_agents: u32,
    pub max_tokens_per_day: u64,
}

impl Default for HostCaps {
    fn default() -> Self {
        Self {
            max_agents: 3,
            max_tokens_per_day: 10_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Tokens {
    pub fn total(self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
    }
}

impl std::ops::Add for Tokens {
    type Output = Self;

    /// Saturates rather than wraps: these are counts from untrusted files.
    fn add(self, other: Self) -> Self {
        Self {
            input: self.input.saturating_add(other.input),
            output: self.output.saturating_add(other.output),
            cache_read: self.cache_read.saturating_add(other.cache_read),
            cache_write: self.cache_write.saturating_add(other.cache_write),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScriptState {
    /// New or changed since it was approved on this host; it does nothing.
    AwaitingApproval,
    Active,
    Paused,
    Disabled,
    /// It does not load or plan; `reason` says why.
    Invalid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptStatus {
    /// The file, which identifies the script on its host.
    pub path: String,
    pub name: String,
    /// The repository it belongs to; `None` for a personal script.
    pub repo: Option<String>,
    pub hash: String,
    pub state: ScriptState,
    /// Why it is paused or invalid.
    pub reason: Option<String>,
    pub caps: Option<ScriptCaps>,
    pub triggers: Vec<TriggerStatus>,
    /// The plan as `check` prints it.
    pub plan: Option<String>,
    pub last_run: Option<RunSummary>,
    /// The run going on now, if any.
    pub running: Option<i64>,
    /// `None` when no agent of the script has known token counts today.
    pub tokens_today: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptCaps {
    pub agents_per_run: u32,
    pub tokens_per_day: u64,
    pub wall_time_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerStatus {
    pub index: usize,
    pub name: Option<String>,
    /// As people read it: `daily at 03:00`, `every 6h`, `manual only`.
    pub schedule: String,
    pub zone: String,
    /// When it fires next; `None` for manual triggers and inactive scripts.
    pub next: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    /// Its schedule came round.
    Schedule,
    /// Its schedule came round while the daemon was down.
    Late,
    /// Someone started it.
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunState {
    Running,
    Completed,
    /// A checked step failed.
    Stopped,
    /// The daemon stopped it: a cap, a stop request, or a step off its plan.
    Halted,
    /// The script errored or ran past a budget.
    Failed,
    /// The daemon went away while it ran; it is not resumed.
    Interrupted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSummary {
    pub id: i64,
    pub script: String,
    pub name: String,
    pub trigger: usize,
    pub trigger_name: Option<String>,
    pub reason: Reason,
    /// The occurrence a scheduled run stands for.
    pub scheduled: Option<i64>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub state: RunState,
    /// Why it stopped, halted or failed.
    pub detail: Option<String>,
    pub steps: u32,
    /// The step running now, counting from 1.
    pub current_step: Option<u32>,
    pub agents: u32,
    pub tokens: Option<u64>,
}

/// `herdr-automate runs show ID --json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunDetail {
    pub run: RunSummary,
    pub steps: Vec<StepRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StepState {
    Running,
    Succeeded,
    Failed,
    Halted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRecord {
    /// Counting from 1, as the plan numbers steps.
    pub position: u32,
    /// The step's name: `agent`, `pull`, `sh`, ...
    pub kind: String,
    /// The step as the plan reads it, tokens filled in.
    pub summary: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub state: StepState,
    pub exit: Option<i32>,
    /// The end of its output, at most a few kilobytes.
    pub log_tail: String,
    pub worktree: Option<String>,
    /// The Herdr pane an agent runs in.
    pub pane: Option<String>,
    pub usage: Usage,
}

/// What a step or run used. `None` means not measured, never zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub tokens: Option<Tokens>,
    pub cpu_ms: Option<u64>,
    pub peak_memory: Option<u64>,
}

/// `herdr-automate usage --json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageReport {
    pub since: i64,
    pub by: String,
    /// Largest token use first.
    pub rows: Vec<UsageRow>,
    pub total: Usage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageRow {
    /// What the row adds up: a script path, a run id, or a step.
    pub key: String,
    pub label: String,
    pub runs: u32,
    pub usage: Usage,
}

/// `herdr-automate log ... --json`: a bounded slice of a log.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogText {
    pub text: String,
    /// The text is the end of a longer log.
    pub truncated: bool,
}

/// The answer to a command that changes something.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Done {
    pub ok: bool,
    pub message: String,
    /// The run a `run-now` queued or started, when known.
    pub run: Option<i64>,
}
