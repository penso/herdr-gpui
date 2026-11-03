//! The plan a person reviews before approving a script, and the guard that
//! keeps a real run on it.
//!
//! A script cannot read the clock, the firing's date, or what a step
//! printed (those reach it as tokens), and every run starts from a fresh
//! state, so the only thing its path can depend on is which steps
//! succeeded. A failing checked step stops the run; an unchecked one hands
//! `ok = false` to the script. The planner runs each trigger once per
//! combination of unchecked outcomes, so the plan is every path the script
//! can take, and the runner's [`Walker`] halts any run that leaves it.

use crate::{
    Caps, Error, Limits, Result,
    schedule::{Schedule, Zone},
    source::Source,
    step::{Call, Verdict},
};
use serde::Serialize;
use std::{convert::Infallible, fmt};

/// The most paths one trigger may have; past it, `check` asks for fewer
/// unchecked steps rather than printing a plan nobody can review.
pub const MAX_PATHS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub script: String,
    /// The source hash approval is pinned to.
    pub hash: String,
    pub caps: Caps,
    pub triggers: Vec<TriggerPlan>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TriggerPlan {
    pub name: Option<String>,
    pub schedule: Schedule,
    pub zone: Zone,
    /// The steps when every one succeeds, each with what failing does.
    pub steps: Vec<PlannedStep>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlannedStep {
    #[serde(flatten)]
    pub call: Call,
    pub on_failure: Failure,
}

/// What the run does when this step fails.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "then", content = "steps")]
pub enum Failure {
    /// The step was checked: the run stops there.
    Stops,
    /// The script reads the failure and goes on exactly as on success.
    Continues,
    /// The script takes these steps instead, possibly none.
    Branch(Vec<PlannedStep>),
}

impl Plan {
    pub fn build(source: &Source, limits: Limits) -> Result<Self> {
        let script = source.load(limits)?;
        let caps = script.caps().clone();
        let triggers = script
            .triggers()
            .iter()
            .map(|trigger| {
                (
                    trigger.name.clone(),
                    trigger.schedule.clone(),
                    trigger.zone.clone(),
                )
            })
            .collect::<Vec<_>>();
        drop(script);
        let mut plans = Vec::with_capacity(triggers.len());
        for (index, (name, schedule, zone)) in triggers.into_iter().enumerate() {
            let mut paths = 0;
            let steps = explore(source, limits, index, &[], &mut paths)?;
            plans.push(TriggerPlan {
                name,
                schedule,
                zone,
                steps,
            });
        }
        Ok(Self {
            script: source.name.clone(),
            hash: source.hash.clone(),
            caps,
            triggers: plans,
        })
    }
}

/// The steps from `decisions.len()` on, when the steps before had those
/// outcomes and every later one succeeds.
fn explore(
    source: &Source,
    limits: Limits,
    trigger: usize,
    decisions: &[bool],
    paths: &mut usize,
) -> Result<Vec<PlannedStep>> {
    *paths += 1;
    if *paths > MAX_PATHS {
        return Err(Error::TooManyPaths { limit: MAX_PATHS });
    }
    let calls = simulate(source, limits, trigger, decisions)?;
    let mut steps = Vec::with_capacity(calls.len().saturating_sub(decisions.len()));
    for (position, call) in calls.into_iter().enumerate().skip(decisions.len()) {
        let on_failure = if call.check {
            Failure::Stops
        } else {
            let mut failing = decisions.to_vec();
            failing.resize(position, true);
            failing.push(false);
            Failure::Branch(explore(source, limits, trigger, &failing, paths)?)
        };
        steps.push(PlannedStep { call, on_failure });
    }
    for index in 0..steps.len() {
        let same = match (&steps[index].on_failure, steps.get(index + 1..)) {
            (Failure::Branch(instead), Some(rest)) => instead.as_slice() == rest,
            _ => false,
        };
        if same && let Some(step) = steps.get_mut(index) {
            step.on_failure = Failure::Continues;
        }
    }
    Ok(steps)
}

/// The calls one run makes, in a fresh state, with the given outcomes.
fn simulate(
    source: &Source,
    limits: Limits,
    trigger: usize,
    decisions: &[bool],
) -> Result<Vec<Call>> {
    let mut calls = Vec::new();
    source.load(limits)?.run::<Infallible>(trigger, |call| {
        let failing = decisions.get(calls.len()) == Some(&false);
        calls.push(call.clone());
        if failing {
            Verdict::Failed
        } else {
            Verdict::Succeeded
        }
    })?;
    Ok(calls)
}

/// Follows a real run through the approved plan. The runner asks before
/// each step whether it is the one the plan has next, and halts the run if
/// not; that also catches any nondeterminism the sandbox missed.
#[derive(Clone, Debug)]
pub struct Walker<'a> {
    steps: &'a [PlannedStep],
    at: usize,
}

impl<'a> Walker<'a> {
    pub fn new(trigger: &'a TriggerPlan) -> Self {
        Self {
            steps: &trigger.steps,
            at: 0,
        }
    }

    /// Whether `call` is the step the plan has next.
    pub fn expects(&self, call: &Call) -> bool {
        self.steps
            .get(self.at)
            .is_some_and(|step| step.call == *call)
    }

    /// Moves past the current step, which succeeded or failed.
    pub fn record(&mut self, ok: bool) {
        let Some(step) = self.steps.get(self.at) else {
            return;
        };
        if ok {
            self.at += 1;
            return;
        }
        match &step.on_failure {
            Failure::Stops => {
                self.steps = &[];
                self.at = 0;
            }
            Failure::Continues => self.at += 1,
            Failure::Branch(instead) => {
                self.steps = instead;
                self.at = 0;
            }
        }
    }
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.script)?;
        writeln!(f, "  caps: {}", self.caps)?;
        for trigger in &self.triggers {
            f.write_str("  ")?;
            if let Some(name) = &trigger.name {
                write!(f, "{name}: ")?;
            }
            write!(f, "{}", trigger.schedule)?;
            if matches!(
                trigger.schedule,
                Schedule::At { .. } | Schedule::Cron { .. }
            ) {
                write!(f, " ({})", trigger.zone)?;
            }
            writeln!(f)?;
            if trigger.steps.is_empty() {
                writeln!(f, "    (no steps)")?;
            }
            write_steps(f, &trigger.steps, 4)?;
        }
        Ok(())
    }
}

fn write_steps(f: &mut fmt::Formatter<'_>, steps: &[PlannedStep], indent: usize) -> fmt::Result {
    let pad = " ".repeat(indent);
    for (index, step) in steps.iter().enumerate() {
        let number = format!("{}.", index + 1);
        writeln!(f, "{pad}{number} {}", step.call)?;
        let under = " ".repeat(indent + number.len() + 1);
        // Everything that runs is shown before it is approved: the summary
        // shortens, so the whole text follows, every line escaped.
        if let Some((label, text)) = step.call.full_text() {
            writeln!(f, "{under}┆ {label}:")?;
            for line in text.split('\n') {
                writeln!(f, "{under}┆ {}", crate::visible(line))?;
            }
        }
        match &step.on_failure {
            Failure::Stops => {}
            Failure::Continues => {
                writeln!(f, "{under}└ on failure: continues with the steps below")?
            }
            Failure::Branch(instead) if instead.is_empty() => {
                writeln!(f, "{under}└ on failure: the run ends")?;
            }
            Failure::Branch(instead) => {
                writeln!(f, "{under}└ on failure, instead:")?;
                write_steps(f, instead, indent + number.len() + 3)?;
            }
        }
    }
    Ok(())
}
