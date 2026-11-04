//! Every string a host's daemon reports, made safe to draw once, when it is
//! read: control and direction-override characters removed, and each field
//! cut to a bound that fits what it holds. A plan and a step's output keep
//! their line breaks. A plan longer than its bound is dropped rather than
//! cut, since approving shows the whole plan the hash covers or nothing.

use crate::notifications::unsafe_char;
use sheprd_automation::report::{
    RunDetail, RunSummary, ScriptStatus, Status, StepRecord, TriggerStatus,
};

/// A name: a script's, a trigger's, a step kind.
const NAME: usize = 256;
/// A file path or a repository.
const PATH: usize = 4096;
/// A reason, a failure's detail, a step's summary.
const REASON: usize = 2000;
/// A version, a time zone, a pane ID.
const SHORT: usize = 128;
/// The longest plan kept; the daemon's are a few to tens of KiB.
pub(crate) const PLAN: usize = 256 * 1024;
/// The end of a step's output kept.
const TAIL: usize = 64 * 1024;

/// One line of text: no control characters, at most `limit` characters.
pub(crate) fn line(text: &str, limit: usize) -> String {
    crate::notifications::safe_text(text, limit)
}

/// Several lines: line breaks kept, other control characters removed, the
/// first `limit` characters.
pub(crate) fn lines(text: &str, limit: usize) -> String {
    text.chars()
        .filter(|c| *c == '\n' || *c == '\t' || !unsafe_char(*c))
        .take(limit)
        .collect()
}

/// [`lines`], keeping the last `limit` characters: the end of an output.
pub(crate) fn tail(text: &str, limit: usize) -> String {
    let kept: Vec<char> = text
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !unsafe_char(*c))
        .collect();
    kept[kept.len().saturating_sub(limit)..].iter().collect()
}

fn opt(text: Option<String>, limit: usize) -> Option<String> {
    text.map(|text| line(&text, limit))
}

/// A plan, whole or not at all.
pub(crate) fn plan(plan: Option<String>) -> Option<String> {
    plan.filter(|plan| plan.chars().count() <= PLAN)
        .map(|plan| lines(&plan, PLAN))
}

pub(crate) fn status(status: Status) -> Status {
    let mut daemon = status.daemon;
    daemon.version = opt(daemon.version, SHORT);
    Status {
        version: line(&status.version, SHORT),
        daemon,
        scripts: status.scripts.into_iter().map(script).collect(),
        runs: status.runs.into_iter().map(run).collect(),
        ..status
    }
}

fn script(script: ScriptStatus) -> ScriptStatus {
    ScriptStatus {
        path: line(&script.path, PATH),
        name: line(&script.name, NAME),
        repo: opt(script.repo, PATH),
        hash: line(&script.hash, SHORT),
        reason: opt(script.reason, REASON),
        triggers: script.triggers.into_iter().map(trigger).collect(),
        plan: plan(script.plan),
        last_run: script.last_run.map(run),
        ..script
    }
}

fn trigger(trigger: TriggerStatus) -> TriggerStatus {
    TriggerStatus {
        name: opt(trigger.name, NAME),
        schedule: line(&trigger.schedule, NAME),
        zone: line(&trigger.zone, SHORT),
        ..trigger
    }
}

fn run(run: RunSummary) -> RunSummary {
    RunSummary {
        script: line(&run.script, PATH),
        name: line(&run.name, NAME),
        trigger_name: opt(run.trigger_name, NAME),
        detail: opt(run.detail, REASON),
        ..run
    }
}

pub(crate) fn detail(detail: RunDetail) -> RunDetail {
    RunDetail {
        run: run(detail.run),
        steps: detail.steps.into_iter().map(step).collect(),
    }
}

fn step(step: StepRecord) -> StepRecord {
    StepRecord {
        kind: line(&step.kind, NAME),
        summary: line(&step.summary, REASON),
        log_tail: tail(&step.log_tail, TAIL),
        worktree: opt(step.worktree, PATH),
        pane: opt(step.pane, SHORT),
        ..step
    }
}
