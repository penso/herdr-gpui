//! Keeps the machine awake on request, and by default the display too, like
//! the Caffeine menu-bar app. `keep_awake_scope = "system"` lets the display
//! sleep.
//!
//! The hold is a `keepawake` guard: IOKit power assertions on macOS, the
//! thread execution state on Windows, and ScreenSaver plus logind inhibitors
//! over D-Bus on Linux. The unsafe FFI stays inside that crate. Each platform
//! drops the hold when the process exits, so a crash or quit releases it
//! without cleanup here. The state is app-wide, so every window's status bar
//! shows the same cup.
//!
//! One dedicated thread owns the guard. Windows ties the execution state to
//! the thread that set it and restores it on drop from that same thread, and
//! Linux makes blocking D-Bus calls that must stay off the UI thread.

use crate::{Result, config::status_bar::AwakeScope};
use gpui::{App, Global, WindowId};
use std::{
    collections::HashSet,
    sync::mpsc::{Receiver, SyncSender},
};

mod error;
mod work;

pub(crate) use error::Error;
pub(crate) use work::Work;

/// How long a hold lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Until the cup is clicked again.
    Indefinite,
    /// Until no window has a working agent.
    UntilAgentsFinish,
}

/// What the status bar draws for the cup.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Cup {
    #[default]
    Off,
    /// A hold or release is in flight; clicks wait for it.
    Pending,
    On(Mode),
}

/// What the worker thread does; the mode is the app's business, not its.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Hold,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    Hold(Mode),
    Release,
}

impl From<Change> for Command {
    fn from(change: Change) -> Self {
        match change {
            Change::Hold(_) => Self::Hold,
            Change::Release => Self::Release,
        }
    }
}

struct Request {
    command: Command,
    /// What a hold keeps awake, as the config said when it was asked for.
    scope: AwakeScope,
    reply: SyncSender<keepawake::Result<()>>,
}

#[derive(Default)]
struct Caffeine {
    cup: Cup,
    /// What the active hold keeps awake: the scope it started with, which a
    /// later config change does not alter until the cup is turned off and on.
    held: AwakeScope,
    /// What the next hold keeps awake, as the config last said.
    scope: AwakeScope,
    /// Counts settled holds and releases, so a timer set after one can tell
    /// whether anyone changed the state since.
    changes: u64,
    /// Dropping it ends the worker, which releases any hold on its own thread.
    worker: Option<SyncSender<Request>>,
    /// Windows whose agents were last seen working.
    working: HashSet<WindowId>,
}

impl Global for Caffeine {}

pub(crate) fn cup(cx: &App) -> Cup {
    cx.try_global::<Caffeine>()
        .map_or(Cup::Off, |caffeine| caffeine.cup)
}

/// Chooses what the next hold keeps awake. Called when the config loads and
/// reloads, so a click right after a Settings change uses the new scope.
pub(crate) fn set_scope(scope: AwakeScope, cx: &mut App) {
    cx.default_global::<Caffeine>().scope = scope;
}

/// What the cup keeps awake now: the active hold's scope, or what the next
/// click would hold while the cup is off.
pub(crate) fn scope(cx: &App) -> AwakeScope {
    cx.try_global::<Caffeine>()
        .map_or_else(AwakeScope::default, |caffeine| match caffeine.cup {
            Cup::Off => caffeine.scope,
            Cup::Pending | Cup::On(_) => caffeine.held,
        })
}

/// How many holds and releases have settled so far.
pub(crate) fn changes(cx: &App) -> u64 {
    cx.try_global::<Caffeine>()
        .map_or(0, |caffeine| caffeine.changes)
}

/// Forgets closed windows, so their last working agents keep nothing awake.
pub(crate) fn install(cx: &mut App) {
    if let Some(appearance) = cx.try_global::<crate::app::InitialAppearance>() {
        let scope = appearance.config.status_bar.keep_awake_scope;
        set_scope(scope, cx);
    }
    cx.on_window_closed(|cx, _| {
        let open = cx.windows();
        cx.default_global::<Caffeine>()
            .working
            .retain(|id| open.iter().any(|window| window.window_id() == *id));
        settle(cx, |error, _| {
            tracing::warn!(category = "caffeine", %error, "could not release the keep-awake hold");
        });
    })
    .detach();
}

/// Starts keeping the machine awake until the cup is clicked again, or stops
/// any hold, and redraws every window. Failures arrive later through
/// `report`, never during this call.
pub(crate) fn toggle(cx: &mut App, report: impl FnOnce(crate::Error, &mut App) + 'static) {
    toggle_with(cx, hold, report);
}

/// Starts keeping the machine awake until every agent finishes, turns an
/// indefinite hold into one, or stops one. With no agent working there is
/// nothing to wait for, which `report` hears about.
pub(crate) fn toggle_until_agents_finish(
    cx: &mut App,
    report: impl FnOnce(crate::Error, &mut App) + 'static,
) {
    toggle_until_agents_finish_with(cx, hold, report);
}

/// Turns a hold until agents finish into one until turned off, as asking
/// for caffeine "on" means; whether it did. The hold itself stays.
pub(crate) fn hold_indefinitely(cx: &mut App) -> bool {
    let caffeine = cx.default_global::<Caffeine>();
    if caffeine.cup != Cup::On(Mode::UntilAgentsFinish) {
        return false;
    }
    caffeine.cup = Cup::On(Mode::Indefinite);
    cx.refresh_windows();
    true
}

/// Records whether `window` has a working agent, and ends an
/// `UntilAgentsFinish` hold once no window does.
pub(crate) fn observe(
    window: WindowId,
    work: Work,
    cx: &mut App,
    report: impl FnOnce(crate::Error, &mut App) + 'static,
) {
    let working = &mut cx.default_global::<Caffeine>().working;
    let finished = match work {
        Work::Working => {
            working.insert(window);
            false
        }
        Work::Idle => working.remove(&window),
        // A reconnecting endpoint's agents are likely still running.
        Work::Unknown => false,
    };
    if finished {
        settle(cx, report);
    }
}

/// `start` makes the hold on the worker thread; it is used only when that
/// thread is first spawned, so tests can stand in for the real power APIs.
fn toggle_with<G>(
    cx: &mut App,
    start: impl FnMut(AwakeScope) -> keepawake::Result<G> + Send + 'static,
    report: impl FnOnce(crate::Error, &mut App) + 'static,
) {
    match cup(cx) {
        Cup::Pending => {}
        Cup::Off => hold_with(cx, Mode::Indefinite, start, report),
        Cup::On(_) => release(cx, report),
    }
}

fn toggle_until_agents_finish_with<G>(
    cx: &mut App,
    start: impl FnMut(AwakeScope) -> keepawake::Result<G> + Send + 'static,
    report: impl FnOnce(crate::Error, &mut App) + 'static,
) {
    let caffeine = cx.default_global::<Caffeine>();
    let idle = caffeine.working.is_empty();
    match caffeine.cup {
        Cup::Pending => {}
        Cup::On(Mode::UntilAgentsFinish) => release(cx, report),
        _ if idle => cx.defer(move |cx| report(Error::NoWorkingAgents.into(), cx)),
        Cup::On(Mode::Indefinite) => {
            caffeine.cup = Cup::On(Mode::UntilAgentsFinish);
            cx.refresh_windows();
        }
        Cup::Off => hold_with(cx, Mode::UntilAgentsFinish, start, report),
    }
}

fn hold_with<G>(
    cx: &mut App,
    mode: Mode,
    start: impl FnMut(AwakeScope) -> keepawake::Result<G> + Send + 'static,
    report: impl FnOnce(crate::Error, &mut App) + 'static,
) {
    let worker = match cx.default_global::<Caffeine>().worker.take() {
        Some(worker) => Ok(worker),
        None => spawn(start),
    };
    send(cx, Change::Hold(mode), worker, report);
}

fn release(cx: &mut App, report: impl FnOnce(crate::Error, &mut App) + 'static) {
    let caffeine = cx.default_global::<Caffeine>();
    let Some(worker) = caffeine.worker.take() else {
        // No worker holds nothing.
        caffeine.cup = Cup::Off;
        cx.refresh_windows();
        return;
    };
    send(cx, Change::Release, Ok(worker), report);
}

/// Ends an `UntilAgentsFinish` hold when no window has a working agent.
fn settle(cx: &mut App, report: impl FnOnce(crate::Error, &mut App) + 'static) {
    let caffeine = cx.default_global::<Caffeine>();
    if caffeine.cup == Cup::On(Mode::UntilAgentsFinish) && caffeine.working.is_empty() {
        release(cx, report);
    }
}

fn send(
    cx: &mut App,
    change: Change,
    worker: Result<SyncSender<Request>>,
    report: impl FnOnce(crate::Error, &mut App) + 'static,
) {
    let (reply, replies) = std::sync::mpsc::sync_channel(1);
    let scope = cx.default_global::<Caffeine>().scope;
    let sent = worker.and_then(|worker| {
        // Never blocks: `Pending` keeps at most one request in the slot.
        worker
            .send(Request {
                command: change.into(),
                scope,
                reply,
            })
            .map(|()| worker)
            .map_err(|_| Error::WorkerStopped.into())
    });
    let caffeine = cx.default_global::<Caffeine>();
    match sent {
        Ok(worker) => caffeine.worker = Some(worker),
        Err(error) => {
            cx.defer(move |cx| report(error, cx));
            return;
        }
    }
    caffeine.cup = Cup::Pending;
    if matches!(change, Change::Hold(_)) {
        caffeine.held = scope;
    }
    cx.refresh_windows();
    let wait = cx
        .background_executor()
        .spawn(async move { replies.recv() });
    cx.spawn(async move |cx| {
        // A dropped reply means the worker died, e.g. a panicking release.
        let result = match wait.await {
            Ok(result) => result.map_err(|source| Error::KeepAwake(source).into()),
            Err(_) => Err(Error::WorkerStopped.into()),
        };
        cx.update(|cx| finish(cx, change, result, report));
    })
    .detach();
}

fn finish(
    cx: &mut App,
    change: Change,
    result: Result<()>,
    report: impl FnOnce(crate::Error, &mut App) + 'static,
) {
    let caffeine = cx.default_global::<Caffeine>();
    caffeine.cup = match (change, &result) {
        (Change::Hold(mode), Ok(())) => Cup::On(mode),
        // A failed hold holds nothing, and a worker that died mid-release
        // took its guard with it.
        (Change::Hold(_), Err(_)) | (Change::Release, _) => Cup::Off,
    };
    {
        caffeine.changes = caffeine.changes.wrapping_add(1);
    }
    if matches!(result, Err(crate::Error::Caffeine(Error::WorkerStopped))) {
        caffeine.worker = None;
    }
    cx.refresh_windows();
    match result {
        Err(error) => report(error, cx),
        // The agents may have finished while the hold was being made.
        Ok(()) => settle(cx, report),
    }
}

fn spawn<G>(
    start: impl FnMut(AwakeScope) -> keepawake::Result<G> + Send + 'static,
) -> Result<SyncSender<Request>> {
    let (worker, requests) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("herdr-keepawake".into())
        .spawn(move || run(&requests, start))
        .map_err(Error::Thread)?;
    Ok(worker)
}

/// Serves requests until the app drops its sender, then drops any hold here.
fn run<G>(requests: &Receiver<Request>, mut start: impl FnMut(AwakeScope) -> keepawake::Result<G>) {
    let mut guard = None;
    for Request {
        command,
        scope,
        reply,
    } in requests
    {
        let result = match command {
            Command::Hold if guard.is_some() => Ok(()),
            Command::Hold => start(scope).map(|held| guard = Some(held)),
            Command::Release => {
                guard = None;
                Ok(())
            }
        };
        // The waiting task outlives the request unless the app is quitting.
        let _ = reply.send(result);
    }
}

fn hold(scope: AwakeScope) -> keepawake::Result<keepawake::KeepAwake> {
    // Idle, not `sleep`: that one needs AC power and does nothing under
    // Windows Modern Standby, and the lid closing should still sleep. The
    // display is held too unless the scope lets it sleep and lock.
    keepawake::Builder::default()
        .display(scope == AwakeScope::Display)
        .idle(true)
        .reason(match scope {
            AwakeScope::Display => "Herdr keeps the display awake",
            AwakeScope::System => "Herdr keeps the system awake",
        })
        .app_name("Herdr")
        .app_reverse_domain(crate::constants::APP_ID)
        .create()
}

#[cfg(test)]
mod tests;
