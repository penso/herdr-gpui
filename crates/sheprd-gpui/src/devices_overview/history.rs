//! How many agents worked and waited on each device over the last two hours,
//! kept by this window from the snapshots it already receives. Nothing is
//! asked of the daemon and nothing is saved: the history starts when the
//! window does, and a device that was not connected has no count for a step.

use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant, SystemTime},
};

/// One step of the activity chart.
pub(crate) const STEP: Duration = Duration::from_secs(60);
/// Steps kept: two hours.
pub(crate) const STEPS: usize = 120;

/// Agents on one device during a step. Each count is the most seen at once
/// in the step, so a short burst between two samples still shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub working: u16,
    pub blocked: u16,
}

impl Counts {
    fn peak(self, other: Self) -> Self {
        Self {
            working: self.working.max(other.working),
            blocked: self.blocked.max(other.blocked),
        }
    }
}

/// Counts per device id for one step; a device missing from it was not
/// connected.
pub(crate) type Step = HashMap<String, Counts>;

#[derive(Debug, Default)]
pub(crate) struct History {
    /// Finished steps, oldest first, at most [`STEPS`].
    steps: VecDeque<Step>,
    /// The step still being counted and elapsed time within it.
    current: Option<Step>,
    into_step: Duration,
    last_observed: Option<(Instant, SystemTime)>,
}

impl History {
    /// Records what each connected device shows `now`. Steps that passed
    /// with no observation, such as while the machine slept, are recorded
    /// empty so the chart keeps its time axis.
    pub fn observe<'a>(
        &mut self,
        now: Instant,
        wall: SystemTime,
        devices: impl IntoIterator<Item = (&'a str, Counts)>,
    ) {
        let elapsed = self.last_observed.replace((now, wall)).map_or(
            Duration::ZERO,
            |(instant, previous_wall)| {
                let awake = now.saturating_duration_since(instant);
                let passed = wall.duration_since(previous_wall).unwrap_or_default();
                // Like the wake detector, ignore small clock slews. A forward
                // adjustment of five seconds or more looks like sleep; a
                // backward adjustment must not freeze or rewind the history.
                if passed.saturating_sub(awake) >= Duration::from_secs(5) {
                    passed
                } else {
                    awake
                }
            },
        );
        let elapsed = self.into_step.saturating_add(elapsed);
        let step = self.current.get_or_insert_with(Step::new);
        self.into_step = elapsed;
        if elapsed >= STEP {
            let finished = std::mem::take(step);
            let passed = elapsed.as_nanos() / STEP.as_nanos();
            // The new step began on the step clock, not at this observation.
            let into = elapsed.as_nanos() % STEP.as_nanos();
            self.into_step = Duration::from_nanos(u64::try_from(into).unwrap_or(0));
            self.push(finished);
            // Up to a full history of empty steps, which pushes out
            // everything older than the sleep.
            let empty = usize::try_from(passed.saturating_sub(1)).unwrap_or(STEPS);
            for _ in 0..empty.min(STEPS) {
                self.push(Step::new());
            }
        }
        let Some(step) = self.current.as_mut() else {
            return;
        };
        // A device already seen this step is updated in place, so most ticks
        // allocate nothing.
        for (id, counts) in devices {
            match step.get_mut(id) {
                Some(seen) => *seen = seen.peak(counts),
                None => {
                    step.insert(id.to_owned(), counts);
                }
            }
        }
    }

    fn push(&mut self, step: Step) {
        if self.steps.len() == STEPS {
            self.steps.pop_front();
        }
        self.steps.push_back(step);
    }

    /// Every step oldest first, ending with the one still being counted.
    pub fn steps(&self) -> impl Iterator<Item = &Step> {
        self.steps.iter().chain(self.current.as_ref())
    }

    /// All devices' agents per step, oldest first.
    pub fn totals(&self) -> Vec<Counts> {
        self.steps()
            .map(|step| {
                step.values().fold(Counts::default(), |sum, counts| Counts {
                    working: sum.working.saturating_add(counts.working),
                    blocked: sum.blocked.saturating_add(counts.blocked),
                })
            })
            .collect()
    }

    /// One device's agents per step, oldest first; None where it was not
    /// connected.
    pub fn lane(&self, id: &str) -> Vec<Option<Counts>> {
        self.steps().map(|step| step.get(id).copied()).collect()
    }
}
