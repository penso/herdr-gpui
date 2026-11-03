//! Rows as Rust values, and the names states are stored under. Unknown
//! names read back as the most cautious state rather than failing a read.

use super::count;
use rusqlite::Row;
use sheprd_automation::report::{Reason, RunState, ScriptCaps, StepState, Tokens, TriggerStatus};

pub(super) const RUN_COLUMNS: &str = "id, path, name, trigger, trigger_name, reason, scheduled, hash, started_at, finished_at, state, detail, steps";
pub(super) const STEP_COLUMNS: &str = "run, position, kind, summary, started_at, finished_at, state, exit, log_tail, worktree, pane, cpu_ms, peak_memory";
pub(super) const AGENT_COLUMNS: &str = "run, position, path, harness, pane, cwd, started_at, done_at, \
     input, output, cache_read, cache_write, cpu_ms, peak_memory, sampled_at";

/// A script as the daemon last loaded it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptRow {
    pub path: String,
    pub name: String,
    pub repo: Option<String>,
    pub hash: String,
    /// Why it does not load or plan.
    pub error: Option<String>,
    /// The plan as `check` prints it.
    pub plan: Option<String>,
    pub triggers: Vec<TriggerStatus>,
    pub caps: Option<ScriptCaps>,
    pub seen_at: i64,
}

impl ScriptRow {
    pub(super) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            path: row.get(0)?,
            name: row.get(1)?,
            repo: row.get(2)?,
            hash: row.get(3)?,
            error: row.get(4)?,
            plan: row.get(5)?,
            // Written by this module, so a bad value is a damaged database:
            // it reads as unknown rather than failing every status.
            triggers: row
                .get::<_, Option<String>>(6)?
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default(),
            caps: row
                .get::<_, Option<String>>(7)?
                .and_then(|json| serde_json::from_str(&json).ok()),
            seen_at: row.get(8)?,
        })
    }
}

/// The daemon's heartbeat row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DaemonRow {
    pub pid: Option<u32>,
    pub version: Option<String>,
    pub started_at: Option<i64>,
    pub beat_at: Option<i64>,
    pub paused_all: bool,
}

/// What the user chose for a script.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Switch {
    pub paused: bool,
    pub disabled: bool,
    pub reason: Option<String>,
}

pub struct NewRun {
    pub path: String,
    pub name: String,
    pub trigger: usize,
    pub trigger_name: Option<String>,
    pub reason: Reason,
    /// The occurrence it stands for; `None` for a manual run, which has no
    /// firing to keep unique.
    pub scheduled: Option<i64>,
    pub hash: String,
    pub started_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRow {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub trigger: usize,
    pub trigger_name: Option<String>,
    pub reason: Reason,
    pub scheduled: Option<i64>,
    pub hash: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub state: RunState,
    pub detail: Option<String>,
    pub steps: u32,
}

impl RunRow {
    pub(super) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            path: row.get(1)?,
            name: row.get(2)?,
            trigger: usize::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
            trigger_name: row.get(4)?,
            reason: reason_from_name(&row.get::<_, String>(5)?),
            scheduled: row.get(6)?,
            hash: row.get(7)?,
            started_at: row.get(8)?,
            finished_at: row.get(9)?,
            state: run_state_from_name(&row.get::<_, String>(10)?),
            detail: row.get(11)?,
            steps: u32::try_from(row.get::<_, i64>(12)?).unwrap_or(0),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepRow {
    pub run: i64,
    pub position: u32,
    pub kind: String,
    pub summary: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub state: StepState,
    pub exit: Option<i32>,
    pub log_tail: String,
    pub worktree: Option<String>,
    pub pane: Option<String>,
    pub cpu_ms: Option<u64>,
    pub peak_memory: Option<u64>,
}

impl StepRow {
    pub(super) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            run: row.get(0)?,
            position: row.get(1)?,
            kind: row.get(2)?,
            summary: row.get(3)?,
            started_at: row.get(4)?,
            finished_at: row.get(5)?,
            state: step_state_from_name(&row.get::<_, String>(6)?),
            exit: row.get(7)?,
            log_tail: row.get(8)?,
            worktree: row.get(9)?,
            pane: row.get(10)?,
            cpu_ms: count(row, 11)?,
            peak_memory: count(row, 12)?,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentRow {
    pub run: i64,
    pub position: u32,
    pub path: String,
    pub harness: String,
    pub pane: String,
    /// The worktree it works in, which finds its session files.
    pub cwd: String,
    pub started_at: i64,
    /// When it was last seen working; `None` while it may still run.
    pub done_at: Option<i64>,
    pub tokens: Option<Tokens>,
    pub cpu_ms: Option<u64>,
    pub peak_memory: Option<u64>,
    pub sampled_at: Option<i64>,
}

impl AgentRow {
    pub(super) fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        let input = count(row, 8)?;
        let tokens = input.map(|input| -> rusqlite::Result<Tokens> {
            Ok(Tokens {
                input,
                output: count(row, 9)?.unwrap_or(0),
                cache_read: count(row, 10)?.unwrap_or(0),
                cache_write: count(row, 11)?.unwrap_or(0),
            })
        });
        Ok(Self {
            run: row.get(0)?,
            position: row.get(1)?,
            path: row.get(2)?,
            harness: row.get(3)?,
            pane: row.get(4)?,
            cwd: row.get(5)?,
            started_at: row.get(6)?,
            done_at: row.get(7)?,
            tokens: tokens.transpose()?,
            cpu_ms: count(row, 12)?,
            peak_memory: count(row, 13)?,
            sampled_at: row.get(14)?,
        })
    }
}

/// What a command asks the daemon to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestKind {
    /// Start a trigger by hand: its name or 1-based number, or the first.
    RunNow {
        path: String,
        trigger: Option<String>,
    },
    /// Stop a running run.
    Stop { run: i64 },
    /// Stop the daemon once its runs have stopped.
    Shutdown,
}

impl RequestKind {
    pub(super) fn columns(&self) -> (&'static str, Option<String>, Option<String>) {
        match self {
            Self::RunNow { path, trigger } => ("run-now", Some(path.clone()), trigger.clone()),
            Self::Stop { run } => ("stop", None, Some(run.to_string())),
            Self::Shutdown => ("shutdown", None, None),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: i64,
    pub kind: RequestKind,
    pub created_at: i64,
}

impl Request {
    /// `None` for a request this version does not know.
    pub(super) fn read(row: &Row<'_>) -> rusqlite::Result<Option<Self>> {
        let kind = match (
            row.get::<_, String>(1)?.as_str(),
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
        ) {
            ("run-now", Some(path), trigger) => RequestKind::RunNow { path, trigger },
            ("stop", _, Some(run)) => match run.parse() {
                Ok(run) => RequestKind::Stop { run },
                Err(_) => return Ok(None),
            },
            ("shutdown", _, _) => RequestKind::Shutdown,
            _ => return Ok(None),
        };
        Ok(Some(Self {
            id: row.get(0)?,
            kind,
            created_at: row.get(4)?,
        }))
    }
}

pub(crate) fn reason_name(reason: Reason) -> &'static str {
    match reason {
        Reason::Schedule => "schedule",
        Reason::Late => "late",
        Reason::Manual => "manual",
    }
}

pub(crate) fn reason_from_name(name: &str) -> Reason {
    match name {
        "schedule" => Reason::Schedule,
        "late" => Reason::Late,
        _ => Reason::Manual,
    }
}

pub(crate) fn run_state_name(state: RunState) -> &'static str {
    match state {
        RunState::Running => "running",
        RunState::Completed => "completed",
        RunState::Stopped => "stopped",
        RunState::Halted => "halted",
        RunState::Failed => "failed",
        RunState::Interrupted => "interrupted",
    }
}

pub(crate) fn run_state_from_name(name: &str) -> RunState {
    match name {
        "running" => RunState::Running,
        "completed" => RunState::Completed,
        "stopped" => RunState::Stopped,
        "halted" => RunState::Halted,
        "interrupted" => RunState::Interrupted,
        _ => RunState::Failed,
    }
}

pub(crate) fn step_state_name(state: StepState) -> &'static str {
    match state {
        StepState::Running => "running",
        StepState::Succeeded => "succeeded",
        StepState::Failed => "failed",
        StepState::Halted => "halted",
    }
}

pub(crate) fn step_state_from_name(name: &str) -> StepState {
    match name {
        "running" => StepState::Running,
        "succeeded" => StepState::Succeeded,
        "halted" => StepState::Halted,
        _ => StepState::Failed,
    }
}
