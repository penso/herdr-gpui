//! One run of one trigger, on its own thread with its own store connection
//! and a freshly loaded script. Before each step it checks the approved plan,
//! the wall time, a stop request and the caps; then it fills in the step's
//! tokens, runs it, and records it.

use crate::{
    Result,
    fingerprint::WorktreeScripts,
    herdr::Herdr,
    paths::Paths,
    settings::Settings,
    steps::{self, StepContext},
    store::{StepRow, Store, Switch},
};
use sheprd_automation::{
    End, Kind, Limits, Plan, Source, Values, Verdict, Walker,
    report::{RunState, StepState},
};
use std::{
    cell::Cell,
    fmt,
    path::PathBuf,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

/// A firing the daemon starts.
pub struct Job {
    pub run: i64,
    pub path: String,
    pub repo: Option<PathBuf>,
    pub source: Source,
    /// The worktree scripts read when the run started, which `run` steps
    /// use: the approved copy, never one changed during the run.
    pub worktree: Option<WorktreeScripts>,
    pub trigger: usize,
    /// `{date}` and `{time}` in the trigger's zone.
    pub date: String,
    pub time: String,
    pub stop: Arc<AtomicBool>,
}

/// Why the runner stopped a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Halt {
    Stopped,
    WallTime,
    OffPlan,
    AgentsPerRun,
    HostAgents,
    ScriptTokens,
    HostTokens,
}

impl fmt::Display for Halt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Stopped => "stopped on request",
            Self::WallTime => "ran past its wall time cap",
            Self::OffPlan => "called a step its approved plan does not have next",
            Self::AgentsPerRun => "reached its agents-per-run cap",
            Self::HostAgents => "the host already runs as many automated agents as its cap allows",
            Self::ScriptTokens => "reached its daily token budget; the script is paused",
            Self::HostTokens => "the host reached its daily token budget",
        })
    }
}

/// Agents that passed the host's cap but are not recorded yet, across the
/// daemon's runs. Checking the cap counts them, so two runs starting agents
/// at once cannot both take the last slot.
#[derive(Debug, Default)]
pub struct Slots(Mutex<u32>);

/// A slot held for one agent until its row is recorded or its start fails.
#[must_use]
pub struct Reservation(Arc<Slots>);

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut starting = self.0.0.lock().unwrap_or_else(PoisonError::into_inner);
        *starting = starting.saturating_sub(1);
    }
}

/// How a run ended, for the store.
pub struct Ended {
    pub state: RunState,
    pub detail: Option<String>,
}

pub fn execute(
    paths: &Paths,
    settings: &Settings,
    herdr: &Herdr,
    slots: &Arc<Slots>,
    job: &Job,
) -> Result<Ended> {
    let store = Store::open(&paths.database())?;
    let plan = Plan::build(&job.source, Limits::default())?;
    let Some(trigger) = plan.triggers.get(job.trigger) else {
        return Ok(Ended {
            state: RunState::Failed,
            detail: Some(format!("the script has no trigger {}", job.trigger + 1)),
        });
    };
    let mut walker = Walker::new(trigger);
    let deadline = Instant::now() + plan.caps.wall_time;
    let reserved = Cell::new(None);
    let context = StepContext {
        paths,
        store: &store,
        herdr,
        run: job.run,
        script_path: &job.path,
        script_name: &job.source.name,
        repo: job.repo.as_deref(),
        worktree: job.worktree.as_ref(),
        stop: &job.stop,
        reserved: &reserved,
        now,
    };
    let mut log_tails = Vec::<Option<String>>::new();
    let mut run_ids = Vec::<Option<String>>::new();
    let mut agents = 0u32;
    let mut failure = None::<String>;
    let end = job
        .source
        .load(Limits::default())?
        .run(job.trigger, |call| {
            let position = u32::try_from(log_tails.len() + 1).unwrap_or(u32::MAX);
            if job.stop.load(Ordering::Relaxed) {
                return Verdict::Halt(Halt::Stopped);
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Verdict::Halt(Halt::WallTime);
            };
            if !walker.expects(call) {
                return Verdict::Halt(Halt::OffPlan);
            }
            if call.step.kind() == Kind::Agent {
                match agent_cap(&store, settings, slots, &plan, &job.path, agents) {
                    Ok(slot) => reserved.set(Some(slot)),
                    Err(halt) => return Verdict::Halt(halt),
                }
                agents += 1;
            }
            let values = Values {
                date: &job.date,
                time: &job.time,
                log_tails: &log_tails,
                run_ids: &run_ids,
            };
            let resolved = call.resolve(&values);
            let summary = resolved.call.to_string();
            let kind = call.step.kind().name();
            let _ = store.start_step(job.run, position, kind, &summary, now());
            let timeout = call
                .timeout
                .map_or(remaining, |timeout| timeout.min(remaining));
            let result = steps::execute(&context, &resolved, position, timeout);
            // A start that failed before its row was recorded frees its slot.
            drop(reserved.take());
            let row = StepRow {
                run: job.run,
                position,
                kind: kind.to_owned(),
                summary,
                started_at: 0,
                finished_at: None,
                state: if result.ok {
                    StepState::Succeeded
                } else {
                    StepState::Failed
                },
                exit: result.exit,
                log_tail: result.log_tail.clone(),
                worktree: result.worktree.clone(),
                pane: result.pane.clone(),
                cpu_ms: result.cpu_ms,
                peak_memory: result.peak_memory,
            };
            let _ = store.finish_step(job.run, position, &row, now());
            if !result.ok && call.check {
                failure = Some(format!("step {position} ({kind}) failed"));
            }
            log_tails.push(Some(result.log_tail));
            run_ids.push(result.run_id);
            walker.record(result.ok);
            // A stop that ended the step ends the run, whatever the step's
            // result would make the script do next.
            if job.stop.load(Ordering::Relaxed) {
                return Verdict::Halt(Halt::Stopped);
            }
            if result.ok {
                Verdict::Succeeded
            } else {
                Verdict::Failed
            }
        });
    Ok(match end {
        Ok(End::Completed) => Ended {
            state: RunState::Completed,
            detail: None,
        },
        Ok(End::Stopped { .. }) => Ended {
            state: RunState::Stopped,
            detail: failure,
        },
        Ok(End::Halted { step, halt }) => {
            if halt == Halt::ScriptTokens {
                let _ = store.set_switch(
                    &job.path,
                    &Switch {
                        paused: true,
                        disabled: false,
                        reason: Some("reached its daily token budget".to_owned()),
                    },
                );
            }
            Ended {
                state: RunState::Halted,
                detail: Some(format!("halted before step {}: {halt}", step + 1)),
            }
        }
        Err(error) => Ended {
            state: RunState::Failed,
            detail: Some(error.to_string()),
        },
    })
}

/// A slot for one more agent, or the cap it would cross.
fn agent_cap(
    store: &Store,
    settings: &Settings,
    slots: &Arc<Slots>,
    plan: &Plan,
    path: &str,
    started: u32,
) -> std::result::Result<Reservation, Halt> {
    if started >= plan.caps.agents_per_run {
        return Err(Halt::AgentsPerRun);
    }
    let today = start_of_day();
    let used = |path: Option<&str>| {
        store
            .tokens_on(path, today)
            .ok()
            .flatten()
            .map(|tokens| tokens.total())
            .unwrap_or(0)
    };
    if used(Some(path)) >= plan.caps.tokens_per_day {
        return Err(Halt::ScriptTokens);
    }
    if used(None) >= settings.caps.max_tokens_per_day {
        return Err(Halt::HostTokens);
    }
    // Held while counting, so the count and the reservation are one step.
    let mut starting = slots.0.lock().unwrap_or_else(PoisonError::into_inner);
    let live = store
        .agents(true)
        .map(|agents| agents.len())
        .unwrap_or(usize::MAX)
        .saturating_add(usize::try_from(*starting).unwrap_or(usize::MAX));
    if live >= usize::try_from(settings.caps.max_agents).unwrap_or(usize::MAX) {
        return Err(Halt::HostAgents);
    }
    *starting += 1;
    Ok(Reservation(slots.clone()))
}

pub fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// Midnight today in the host's zone, as Unix seconds.
pub fn start_of_day() -> i64 {
    jiff::Zoned::now()
        .start_of_day()
        .map(|midnight| midnight.timestamp().as_second())
        .unwrap_or_else(|_| now() - 86_400)
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests;
