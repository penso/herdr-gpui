//! What a script or run looks like in the section: its state icon and
//! color, and the short texts of its row, derived from the daemon's report
//! alone so drawing never asks the host anything.

use super::format;
use crate::{config::Theme, i18n::fl};
use herdr_automation::report::{
    RunState, RunSummary, ScriptCaps, ScriptState, ScriptStatus, Status, StepState,
};

/// The state a row shows, which picks its icon and color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    Running,
    Ok,
    Failed,
    /// Waits for the user: a script to approve.
    Waiting,
    /// Does nothing on its own: paused, disabled, or not run yet.
    Paused,
}

impl Look {
    pub(crate) fn icon(self) -> &'static str {
        match self {
            Self::Running => "icons/status-solid-working.svg",
            Self::Ok => "icons/status-solid-done.svg",
            Self::Failed => "icons/status-solid-blocked.svg",
            Self::Waiting => "icons/status-outline-idle.svg",
            Self::Paused => "icons/status-outline-unknown.svg",
        }
    }

    pub(crate) fn color(self, theme: &Theme) -> u32 {
        match self {
            Self::Running => theme.ink(theme.palette[4]),
            Self::Ok => theme.ink(theme.palette[2]),
            Self::Failed => theme.ink(theme.palette[1]),
            Self::Waiting => theme.ink(theme.palette[3]),
            Self::Paused => theme.muted,
        }
    }

    /// The color of a row's detail text: loud only when it asks for action.
    pub(crate) fn detail_color(self, theme: &Theme) -> u32 {
        match self {
            Self::Failed | Self::Waiting => self.color(theme),
            Self::Running | Self::Ok | Self::Paused => theme.subtext(),
        }
    }
}

/// A run that ended badly: a step failed, the daemon halted it, or the
/// script errored.
pub(crate) fn failed(state: RunState) -> bool {
    matches!(
        state,
        RunState::Failed | RunState::Halted | RunState::Stopped
    )
}

pub(crate) fn run_look(state: RunState) -> Look {
    match state {
        RunState::Running => Look::Running,
        RunState::Completed => Look::Ok,
        RunState::Stopped | RunState::Halted | RunState::Failed => Look::Failed,
        RunState::Interrupted => Look::Paused,
    }
}

pub(crate) fn step_look(state: StepState) -> Look {
    match state {
        StepState::Running => Look::Running,
        StepState::Succeeded => Look::Ok,
        StepState::Failed | StepState::Halted => Look::Failed,
    }
}

pub(crate) fn look(script: &ScriptStatus) -> Look {
    if script.running.is_some() {
        return Look::Running;
    }
    match script.state {
        ScriptState::AwaitingApproval => Look::Waiting,
        ScriptState::Invalid => Look::Failed,
        ScriptState::Paused | ScriptState::Disabled => Look::Paused,
        ScriptState::Active => match &script.last_run {
            Some(run) if failed(run.state) => Look::Failed,
            Some(_) => Look::Ok,
            None => Look::Paused,
        },
    }
}

/// Whether the script waits on the user: to be approved, or after a run
/// that failed. The bar's count adds these up.
pub(crate) fn needs_you(script: &ScriptStatus) -> bool {
    script.state == ScriptState::AwaitingApproval
        || script
            .last_run
            .as_ref()
            .is_some_and(|run| failed(run.state))
}

/// The script's first trigger, as the daemon words it.
pub(crate) fn schedule(script: &ScriptStatus) -> String {
    match script.triggers.as_slice() {
        [] => fl!("automations-no-trigger"),
        [only] => only.schedule.clone(),
        [first, rest @ ..] => format!("{} +{}", first.schedule, rest.len()),
    }
}

/// When the script fires next, across its triggers.
pub(crate) fn next_firing(script: &ScriptStatus) -> Option<i64> {
    script
        .triggers
        .iter()
        .filter_map(|trigger| trigger.next)
        .min()
}

/// The run `id` among the host's recent runs.
pub(crate) fn find_run(status: &Status, id: i64) -> Option<&RunSummary> {
    status.runs.iter().find(|run| run.id == id).or_else(|| {
        status
            .scripts
            .iter()
            .filter_map(|script| script.last_run.as_ref())
            .find(|run| run.id == id)
    })
}

/// How long after a run ends its agents may still be spending: the daemon
/// counts an agent for at most a day.
const AGENTS_SPEND_FOR: i64 = 86_400;

/// Whether a run's page, read as `read`, may no longer say what the host
/// would: the run was still going, its summary in the latest status
/// differs, or its agents may still be using tokens after it ended.
pub(crate) fn run_stale(latest: Option<&RunSummary>, read: Option<&RunSummary>, now: i64) -> bool {
    let running = |run: &RunSummary| run.state == RunState::Running;
    let changed = latest
        .zip(read)
        .is_some_and(|(latest, read)| latest.tokens != read.tokens || latest.state != read.state);
    let spending = read.is_some_and(|read| {
        read.agents > 0
            && read
                .finished_at
                .is_some_and(|ended| now.saturating_sub(ended) < AGENTS_SPEND_FOR)
    });
    latest.is_some_and(running) || read.is_some_and(running) || changed || spending
}

/// What the row says beside the schedule: what runs now, why it fails or
/// waits, or when it fires next.
pub(crate) fn detail(script: &ScriptStatus, status: &Status, now: i64) -> String {
    if let Some(id) = script.running {
        return match find_run(status, id) {
            Some(run) => progress(run, now),
            None => fl!("automations-running"),
        };
    }
    match script.state {
        ScriptState::AwaitingApproval => match &script.reason {
            Some(reason) => fl!("automations-awaiting", reason = reason.as_str()),
            None => fl!("automations-awaiting-new"),
        },
        ScriptState::Invalid => script
            .reason
            .clone()
            .unwrap_or_else(|| fl!("automations-invalid")),
        ScriptState::Paused => script
            .reason
            .clone()
            .unwrap_or_else(|| fl!("automations-paused")),
        ScriptState::Disabled => fl!("automations-disabled"),
        ScriptState::Active => match &script.last_run {
            Some(run) if failed(run.state) => failure(run),
            _ => match next_firing(script) {
                Some(next) => fl!("automations-next", when = format::when(next, now)),
                None => fl!("automations-runs-when-started"),
            },
        },
    }
}

/// Why a failed run failed: the daemon's words, or that it did not finish.
pub(crate) fn failure(run: &RunSummary) -> String {
    match run.detail.as_deref() {
        Some(why) => fl!("automations-failed", why = why),
        None => fl!("automations-failed-unfinished"),
    }
}

/// A script's caps: agents per run, tokens a day, and time per run.
pub(crate) fn caps(caps: ScriptCaps) -> String {
    fl!(
        "automations-caps",
        agents = caps.agents_per_run,
        tokens = format::tokens(caps.tokens_per_day),
        time = format::span(i64::try_from(caps.wall_time_seconds).unwrap_or(i64::MAX))
    )
}

/// A running run's progress: `Step 2/3 · 12m04s`.
pub(crate) fn progress(run: &RunSummary, now: i64) -> String {
    let elapsed = format::span(now - run.started_at);
    match run.current_step {
        Some(step) => fl!(
            "automations-progress-step",
            step = step,
            steps = run.steps,
            elapsed = elapsed
        ),
        None => fl!("automations-progress-starting", elapsed = elapsed),
    }
}

/// How a finished run went, short: `Wed 03:00 ✓ 41m`.
pub(crate) fn result(run: &RunSummary, now: i64) -> String {
    let mark = match run.state {
        RunState::Running => "…",
        RunState::Completed => "✓",
        RunState::Interrupted => "–",
        RunState::Stopped | RunState::Halted | RunState::Failed => "✕",
    };
    let when = format::when(run.started_at, now);
    match run.finished_at {
        Some(end) => fl!(
            "automations-result",
            when = when,
            mark = mark,
            took = format::span(end - run.started_at)
        ),
        None => fl!("automations-result-running", when = when, mark = mark),
    }
}

/// A run's line in a list: how long, how many agents, how many tokens, or
/// why it stopped.
pub(crate) fn summary(run: &RunSummary, now: i64) -> String {
    if run.state == RunState::Running {
        return progress(run, now);
    }
    let mut parts = Vec::new();
    if let Some(detail) = run.detail.as_ref().filter(|_| failed(run.state)) {
        parts.push(detail.clone());
    }
    if let Some(end) = run.finished_at {
        parts.push(format::span(end - run.started_at));
    }
    if run.agents > 0 {
        parts.push(fl!("automations-agents", count = run.agents));
    }
    if let Some(tokens) = run.tokens.filter(|tokens| *tokens > 0) {
        parts.push(fl!("automations-tokens", tokens = format::tokens(tokens)));
    }
    if run.state == RunState::Interrupted {
        parts.push(fl!("automations-interrupted"));
    }
    parts.join(" · ")
}

/// The script's tokens today, or a dash when no agent counted any.
pub(crate) fn tokens_today(script: &ScriptStatus) -> String {
    script
        .tokens_today
        .map_or_else(|| "—".to_owned(), format::tokens)
}
