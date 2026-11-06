//! `caffeine`: report, or hold and release as the status bar cup does,
//! optionally for a while. A timed hold ends on its own unless anyone,
//! here or in a window, changed caffeine after it was set.
use super::{Watches, socket};
use crate::{
    caffeine::{self, Cup, Mode},
    control::protocol::{
        ErrorCode, Response,
        app::Switch,
        settings::{MAX_CAFFEINE_SECONDS, Reply},
    },
};
use gpui::{App, Global};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

/// How long a hold or release may take to settle.
const SETTLE: Duration = Duration::from_secs(5);
/// How often a timed hold looks whether it has settled.
const POLL: Duration = Duration::from_millis(50);

/// A timed hold: released at `until`, unless caffeine has changed since
/// `changes` holds and releases had settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Timer {
    pub changes: u64,
    pub until: Instant,
}

/// The timer in force; a later request replaces or clears it.
#[derive(Default)]
struct Timed(Option<Timer>);

impl Global for Timed {}

/// How long the hold `timer` was set on has left: `None` once anyone
/// changed caffeine after it was set, or the hold is off, so the timer must
/// not touch it; zero when it is due.
pub(super) fn remaining(timer: Timer, changes: u64, cup: Cup, now: Instant) -> Option<Duration> {
    (timer.changes == changes && cup == Cup::On(Mode::Indefinite))
        .then(|| timer.until.saturating_duration_since(now))
}

pub(super) fn state_name(cup: Cup) -> &'static str {
    match cup {
        Cup::Off => "off",
        Cup::Pending => "pending",
        Cup::On(Mode::Indefinite) => "on",
        Cup::On(Mode::UntilAgentsFinish) => "until_agents_finish",
    }
}

fn state(cx: &App) -> Response {
    let cup = caffeine::cup(cx);
    let left = cx
        .try_global::<Timed>()
        .and_then(|timed| timed.0)
        .and_then(|timer| remaining(timer, caffeine::changes(cx), cup, Instant::now()));
    Response::Settings {
        reply: Reply::Caffeine {
            state: state_name(cup).to_owned(),
            seconds: left.map(|left| left.as_secs()),
        },
    }
}

pub(super) fn serve(
    switch: Option<Switch>,
    seconds: Option<u64>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    if seconds.is_some_and(|seconds| seconds == 0 || seconds > MAX_CAFFEINE_SECONDS) {
        incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "--for must be between one second and a day",
        ));
        return;
    }
    let Some(switch) = switch else {
        incoming.respond(state(cx));
        return;
    };
    let cup = caffeine::cup(cx);
    if cup == Cup::Pending {
        incoming.respond(Response::error(
            ErrorCode::Busy,
            "Caffeine is changing; try again",
        ));
        return;
    }
    let on = matches!(cup, Cup::On(_));
    let wanted = switch.apply(on);
    // "On" means until turned off (or for `--for`), so a hold that would end
    // with the agents becomes one that does not.
    if wanted && on {
        caffeine::hold_indefinitely(cx);
    }
    if seconds.is_some() && !wanted {
        incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "--for keeps the machine awake for a while, but this turns caffeine off",
        ));
        return;
    }
    supersede(seconds, cx);
    let failure = Rc::new(RefCell::new(None));
    let before = caffeine::changes(cx);
    let toggled = wanted != on;
    if toggled {
        let failure = failure.clone();
        caffeine::toggle(cx, move |error, _| {
            failure.borrow_mut().replace(error);
        });
    }
    // The timer is armed by a task of its own, so a caller that hangs up
    // before the hold settles can never leave it held for good.
    let armed = Rc::new(std::cell::Cell::new(seconds.is_none()));
    if let Some(seconds) = seconds {
        arm_once_settled(seconds, before, toggled, armed.clone(), cx);
    }
    watches.add(
        incoming,
        SETTLE,
        move |cx| {
            if let Some(error) = failure.borrow_mut().take() {
                return Some(Response::error(ErrorCode::Failed, error.to_string()));
            }
            (settled(before, toggled, cx) && armed.get()).then(|| state(cx))
        },
        Response::error(ErrorCode::Timeout, "Caffeine did not settle in time"),
        cx,
    );
}

/// An accepted request without `--for` cancels any timer: `caffeine on`
/// after `caffeine on --for 1m` holds until turned off, even though the
/// hold itself does not change.
pub(super) fn supersede(seconds: Option<u64>, cx: &mut App) {
    if seconds.is_none() {
        cx.set_global(Timed(None));
    }
}

/// Whether a timer is in force, for tests.
#[cfg(test)]
pub(super) fn timed(cx: &App) -> bool {
    cx.try_global::<Timed>()
        .is_some_and(|timed| timed.0.is_some())
}

#[cfg(test)]
pub(super) fn set_timed(timer: Timer, cx: &mut App) {
    cx.set_global(Timed(Some(timer)));
}

/// Whether the hold or release asked for has settled. A toggle that never
/// reached the worker reports its failure on a later turn, without ever
/// changing the state.
fn settled(before: u64, toggled: bool, cx: &App) -> bool {
    caffeine::cup(cx) != Cup::Pending && !(toggled && caffeine::changes(cx) == before)
}

/// Waits for the hold to settle, up to [`SETTLE`], then times it.
fn arm_once_settled(
    seconds: u64,
    before: u64,
    toggled: bool,
    armed: Rc<std::cell::Cell<bool>>,
    cx: &mut App,
) {
    let clock = cx.background_executor().clone();
    let until = Instant::now() + SETTLE;
    cx.spawn(async move |cx| {
        loop {
            let done = cx.update(|cx| {
                if !settled(before, toggled, cx) {
                    return false;
                }
                let timer = (caffeine::cup(cx) == Cup::On(Mode::Indefinite)).then(|| Timer {
                    changes: caffeine::changes(cx),
                    until: Instant::now() + Duration::from_secs(seconds),
                });
                arm(timer, cx);
                true
            });
            if done || Instant::now() >= until {
                armed.set(true);
                return;
            }
            clock.timer(POLL).await;
        }
    })
    .detach();
}

/// Puts `timer` in force, replacing any other, and ends its hold when due.
fn arm(timer: Option<Timer>, cx: &mut App) {
    cx.set_global(Timed(timer));
    let Some(timer) = timer else {
        return;
    };
    let clock = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        clock
            .timer(timer.until.saturating_duration_since(Instant::now()))
            .await;
        cx.update(|cx| {
            let current = cx.try_global::<Timed>().and_then(|timed| timed.0);
            if current != Some(timer) {
                return;
            }
            match remaining(
                timer,
                caffeine::changes(cx),
                caffeine::cup(cx),
                Instant::now(),
            ) {
                // Timers may wake a little early.
                Some(left) if !left.is_zero() => arm(Some(timer), cx),
                Some(_) => {
                    cx.set_global(Timed(None));
                    caffeine::toggle(cx, |error, _| {
                        tracing::warn!(%error, "Could not end a timed keep-awake");
                    });
                }
                None => cx.set_global(Timed(None)),
            }
        });
    })
    .detach();
}
