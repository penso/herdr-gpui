//! The supervisor's part in updating Sheprd from a phone
//! (`control::app_update`): the window running the updater reports it here,
//! each poll passes that to the daemon, and a phone's request answered by a
//! poll steps the updater through what the update panel's buttons do, up to
//! the restart. The updater still verifies the release it installs.

use super::*;
use sheprd_remote_control::control::app_update::{AppUpdate, MAX_UPDATE_TEXT, Stage};
use sheprd_updater::{Disabled, State, Updater};

/// How often the window's report is renewed when nothing changed.
const RENEW: Duration = Duration::from_secs(1);
/// How long a report counts after its window last renewed it, so the daemon
/// stops hearing about an updater whose window closed.
const REPORT_LIFE: Duration = Duration::from_secs(5);

/// What the updater should do next for a phone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Flow {
    Check,
    Download,
    Install,
    Restart,
    /// The phone's update is over: installed, refused, or nothing newer.
    Stop,
    Upgrade,
    /// The updater is working.
    Wait,
}

/// The next step from `state`. `checked` says this update already took a
/// step, so an idle updater is done with it: nothing newer, or someone at the
/// computer cancelled, which a phone's request must not undo.
fn flow(state: &State, checked: bool) -> Flow {
    match state {
        State::Available { .. } => Flow::Download,
        State::Checking
        | State::Downloading { .. }
        | State::Installing
        | State::Upgrading { .. } => Flow::Wait,
        State::Current | State::Error(_) | State::Idle if !checked => Flow::Check,
        State::Cancelling
        | State::Current
        | State::Disabled(_)
        | State::Error(_)
        | State::Idle
        | State::Restarting => Flow::Stop,
        State::Homebrew { .. } => Flow::Upgrade,
        State::Ready { .. } => Flow::Install,
        State::Restart { .. } => Flow::Restart,
    }
}

/// What `running` becomes after `flow`: any step counts as `checked`.
fn after(flow: Flow, checked: bool) -> Option<bool> {
    match flow {
        Flow::Stop => None,
        Flow::Wait => Some(checked),
        Flow::Check | Flow::Download | Flow::Install | Flow::Restart | Flow::Upgrade => Some(true),
    }
}

/// `text` cut to what the daemon accepts, on a character boundary.
fn bounded(mut text: String) -> String {
    if text.len() > MAX_UPDATE_TEXT {
        let end = (0..=MAX_UPDATE_TEXT)
            .rev()
            .find(|end| text.is_char_boundary(*end))
            .unwrap_or(0);
        text.truncate(end);
    }
    text
}

/// What a phone is told about `updater`. While a phone's update runs, a
/// release waiting to be taken is the next step rather than an offer.
fn report(updater: &Updater, running: bool) -> AppUpdate {
    let (stage, detail) = match updater.state() {
        State::Available { .. }
        | State::Homebrew { .. }
        | State::Ready { .. }
        | State::Restart { .. } => (
            if running {
                Stage::Busy
            } else {
                Stage::Available
            },
            None,
        ),
        State::Cancelling
        | State::Checking
        | State::Downloading { .. }
        | State::Installing
        | State::Restarting
        | State::Upgrading { .. } => (Stage::Busy, None),
        State::Current => (Stage::Current, None),
        State::Disabled(reason) => (Stage::Disabled, Some(reason.to_string())),
        State::Error(failure) => (Stage::Failed, Some(failure.to_string())),
        State::Idle => (Stage::Idle, None),
    };
    AppUpdate {
        stage,
        latest: updater
            .latest_version()
            .map(|version| bounded(version.to_owned())),
        detail: detail.map(bounded),
    }
}

/// The supervisor's update state.
#[derive(Debug, Default)]
pub(crate) struct Updates {
    /// What the updater's window last said, and when.
    reported: Option<(AppUpdate, Instant)>,
    /// A phone's update is running; the flag is `flow`'s `checked`.
    running: Option<bool>,
}

impl Updates {
    /// What the next poll tells the daemon.
    pub(super) fn outgoing(&self) -> Option<AppUpdate> {
        self.reported
            .as_ref()
            .filter(|(_, at)| at.elapsed() < REPORT_LIFE)
            .map(|(update, _)| update.clone())
    }

    /// A poll brought a phone's request. One already running goes on.
    pub(super) fn requested(&mut self) {
        self.running.get_or_insert(false);
    }

    /// Takes the updater one step, then says whether it should go on.
    fn advance(running: &mut Option<bool>, updater: &mut Updater) {
        let Some(checked) = *running else {
            return;
        };
        let next = flow(updater.state(), checked);
        match next {
            Flow::Check => updater.check(),
            Flow::Download => updater.download(),
            Flow::Install => updater.install(),
            Flow::Restart => updater.restart(),
            Flow::Upgrade => updater.upgrade(),
            Flow::Stop | Flow::Wait => {}
        }
        *running = after(next, checked);
    }
}

impl Supervisor {
    /// Called by each window's tick: the window running the updater reports
    /// it and takes a phone's update a step further.
    pub(crate) fn follow_updater(updater: &mut Updater, cx: &mut App) {
        if matches!(updater.state(), State::Disabled(Disabled::Secondary)) {
            return;
        }
        let Some(supervisor) = cx.try_global::<Self>() else {
            return;
        };
        let updates = &supervisor.updates;
        let mut running = updates.running;
        Updates::advance(&mut running, updater);
        let update = report(updater, running.is_some());
        let unchanged = running == updates.running
            && updates
                .reported
                .as_ref()
                .is_some_and(|(known, at)| *known == update && at.elapsed() < RENEW);
        if unchanged {
            return;
        }
        let updates = &mut cx.global_mut::<Self>().updates;
        updates.running = running;
        updates.reported = Some((update, Instant::now()));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
