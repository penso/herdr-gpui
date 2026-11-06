//! Building the reports `--json` prints from the store. They read state
//! only, so they work while the daemon is down.

use crate::{
    Result,
    daemon::{self, Lock},
    paths::Paths,
    runner::start_of_day,
    sampler::usage_of,
    service,
    settings::Settings,
    store::{RunRow, ScriptRow, Store},
};
use sheprd_automation::report::{
    Daemon, PROTOCOL, RECENT_RUNS, RunDetail, RunState, RunSummary, ScriptState, ScriptStatus,
    Status, StepRecord, StepState, Tokens, Usage, UsageReport, UsageRow,
};
use std::collections::BTreeMap;

/// A daemon that has not beaten for this long is taken as gone.
const STALE_BEAT: i64 = 60;

pub fn status(paths: &Paths, store: &Store, settings: &Settings, now: i64) -> Result<Status> {
    let beat = store.daemon()?.unwrap_or_default();
    let fresh = beat.beat_at.is_some_and(|at| now - at <= STALE_BEAT);
    let daemon = Daemon {
        running: fresh && Lock::held(paths),
        pid: beat.pid,
        version: beat.version,
        started_at: beat.started_at,
        beat_at: beat.beat_at,
        paused_all: beat.paused_all,
        service: service::installed(),
    };
    let today = start_of_day();
    let mut scripts = Vec::new();
    for row in store.scripts()? {
        scripts.push(script_status(store, row, today)?);
    }
    let runs = store
        .runs(None, RECENT_RUNS)?
        .iter()
        .map(|run| summary(store, run))
        .collect::<Result<Vec<_>>>()?;
    Ok(Status {
        protocol: PROTOCOL,
        version: daemon::version().to_owned(),
        daemon,
        caps: settings.caps.clone(),
        today: store.tokens_on(None, today)?.unwrap_or_default(),
        scripts,
        runs,
    })
}

fn script_status(store: &Store, row: ScriptRow, today: i64) -> Result<ScriptStatus> {
    let approved = store.approved_hash(&row.path)?.as_deref() == Some(row.hash.as_str());
    let switch = store.switch(&row.path)?;
    let state = if row.error.is_some() || row.plan.is_none() {
        ScriptState::Invalid
    } else if !approved {
        ScriptState::AwaitingApproval
    } else if switch.disabled {
        ScriptState::Disabled
    } else if switch.paused {
        ScriptState::Paused
    } else {
        ScriptState::Active
    };
    let mut triggers = row.triggers;
    if state != ScriptState::Active {
        for trigger in &mut triggers {
            trigger.next = None;
        }
    }
    let last_run = store
        .runs(Some(&row.path), 1)?
        .first()
        .map(|run| summary(store, run))
        .transpose()?;
    Ok(ScriptStatus {
        running: store.running(&row.path)?,
        tokens_today: store.tokens_on(Some(&row.path), today)?.map(Tokens::total),
        reason: row.error.or(match state {
            ScriptState::Paused | ScriptState::Disabled => switch.reason,
            ScriptState::AwaitingApproval => Some(if store.approved_hash(&row.path)?.is_some() {
                "changed since it was approved".to_owned()
            } else {
                "new; not approved on this host yet".to_owned()
            }),
            _ => None,
        }),
        path: row.path,
        name: row.name,
        repo: row.repo,
        hash: row.hash,
        state,
        caps: row.caps,
        triggers,
        plan: row.plan,
        last_run,
    })
}

pub fn summary(store: &Store, run: &RunRow) -> Result<RunSummary> {
    let agents = store.run_agents(run.id)?;
    let tokens = agents
        .iter()
        .filter_map(|agent| agent.tokens)
        .reduce(|sum, tokens| sum + tokens)
        .map(Tokens::total);
    let current_step = if run.state == RunState::Running {
        store
            .steps(run.id)?
            .iter()
            .rev()
            .find(|step| step.state == StepState::Running)
            .map(|step| step.position)
    } else {
        None
    };
    Ok(RunSummary {
        id: run.id,
        script: run.path.clone(),
        name: run.name.clone(),
        trigger: run.trigger,
        trigger_name: run.trigger_name.clone(),
        reason: run.reason,
        scheduled: run.scheduled,
        started_at: run.started_at,
        finished_at: run.finished_at,
        state: run.state,
        detail: run.detail.clone(),
        steps: run.steps,
        current_step,
        agents: u32::try_from(agents.len()).unwrap_or(u32::MAX),
        tokens,
    })
}

pub fn run_detail(store: &Store, id: i64) -> Result<RunDetail> {
    let run = store.run(id)?.ok_or(crate::Error::NoRun(id))?;
    let agents = store.run_agents(id)?;
    let steps = store
        .steps(id)?
        .into_iter()
        .map(|step| {
            let agent = agents.iter().find(|agent| agent.position == step.position);
            StepRecord {
                position: step.position,
                kind: step.kind,
                summary: step.summary,
                started_at: step.started_at,
                finished_at: step.finished_at,
                state: step.state,
                exit: step.exit,
                log_tail: step.log_tail,
                worktree: step.worktree,
                pane: step.pane,
                usage: agent.map(usage_of).unwrap_or(Usage {
                    tokens: None,
                    cpu_ms: step.cpu_ms,
                    peak_memory: step.peak_memory,
                }),
            }
        })
        .collect();
    Ok(RunDetail {
        run: summary(store, &run)?,
        steps,
    })
}

/// How usage is added up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    Script,
    Run,
    Step,
}

impl By {
    pub fn name(self) -> &'static str {
        match self {
            Self::Script => "script",
            Self::Run => "run",
            Self::Step => "step",
        }
    }
}

/// What runs since `since` used, for one script or all.
pub fn usage(store: &Store, script: Option<&str>, since: i64, by: By) -> Result<UsageReport> {
    let mut rows = BTreeMap::<String, UsageRow>::new();
    let mut total = Usage::default();
    for run in store.runs(script, usize::MAX)? {
        if run.started_at < since {
            continue;
        }
        let agents = store.run_agents(run.id)?;
        for step in store.steps(run.id)? {
            let agent = agents.iter().find(|agent| agent.position == step.position);
            let used = agent.map(usage_of).unwrap_or(Usage {
                tokens: None,
                cpu_ms: step.cpu_ms,
                peak_memory: step.peak_memory,
            });
            let (key, label) = match by {
                By::Script => (run.path.clone(), run.name.clone()),
                By::Run => (run.id.to_string(), format!("{} #{}", run.name, run.id)),
                By::Step => (
                    format!("{}:{}", run.id, step.position),
                    format!("{} #{} step {}", run.name, run.id, step.position),
                ),
            };
            let row = rows.entry(key.clone()).or_insert_with(|| UsageRow {
                key,
                label,
                runs: 0,
                usage: Usage::default(),
            });
            row.usage = add(row.usage, used);
            total = add(total, used);
        }
        let run_key = match by {
            By::Script => Some(run.path.clone()),
            By::Run => Some(run.id.to_string()),
            By::Step => None,
        };
        if let Some(row) = run_key.and_then(|key| rows.get_mut(&key)) {
            row.runs += 1;
        }
    }
    let mut rows = rows.into_values().collect::<Vec<_>>();
    rows.sort_by_key(|row| std::cmp::Reverse(row.usage.tokens.map(Tokens::total).unwrap_or(0)));
    Ok(UsageReport {
        since,
        by: by.name().to_owned(),
        rows,
        total,
    })
}

/// Sums two usages; unknown stays unknown only when both are.
fn add(a: Usage, b: Usage) -> Usage {
    let sum = |a: Option<u64>, b: Option<u64>| match (a, b) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0).saturating_add(b.unwrap_or(0))),
    };
    Usage {
        tokens: match (a.tokens, b.tokens) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or_default() + b.unwrap_or_default()),
        },
        cpu_ms: sum(a.cpu_ms, b.cpu_ms),
        peak_memory: a.peak_memory.max(b.peak_memory),
    }
}
