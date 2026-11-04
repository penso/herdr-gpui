//! `events` and `wait`. Events are found by comparing, every tick, what the
//! app shows with what it showed the tick before, so no feature has to
//! report them: windows, the app's tabs, kept notes, worktree notes, and
//! host status. The log is bounded; a slow reader misses the oldest.
use super::{Watches, main_windows, socket, window_for};
use crate::{
    browser::Store,
    control::protocol::{
        Call, Response,
        events::{Condition, Event, EventKind, EventsOp, Reply},
    },
};
use gpui::{App, Global};
use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Events kept for readers that fall behind.
const KEEP: usize = 512;
/// Events in one answer.
const BATCH: usize = 256;

/// What the app showed at the last tick.
#[derive(Default, PartialEq)]
struct Seen {
    windows: Vec<u64>,
    active: Option<u64>,
    tabs: BTreeMap<u64, String>,
    /// Kept batches by number, with the pane each is for.
    kept: Vec<(u64, String)>,
    notes: u64,
    /// `(window, host)` to status.
    hosts: BTreeMap<(u64, String), String>,
    /// Cloud batches kept, by number, with their phase.
    batches: BTreeMap<u64, crate::control::protocol::cloud::Phase>,
}

#[derive(Default)]
pub(super) struct Log {
    /// The last sequence number handed out; the first event is 1.
    seq: u64,
    events: VecDeque<Event>,
    seen: Option<Seen>,
}

impl Global for Log {}

/// This app run, so a reader's sequence numbers from an earlier run are
/// told apart: the process ID with the time it first answered.
fn instance() -> u64 {
    static INSTANCE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *INSTANCE.get_or_init(|| {
        let started = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as u64);
        (u64::from(std::process::id()) << 40) ^ started
    })
}

/// Where a poll starts: after its sequence number when it counts in this
/// run; from the start of what this run kept when it counts in another; and
/// from now when it names none.
fn start(after: Option<u64>, instance: Option<u64>, now: u64) -> u64 {
    match (after, instance) {
        (Some(_), Some(theirs)) if theirs != self::instance() => 0,
        (Some(after), _) => after,
        (None, _) => now,
    }
}

impl Log {
    fn push(&mut self, kind: EventKind, subject: impl Into<String>, detail: Option<String>) {
        self.seq += 1;
        if self.events.len() >= KEEP {
            self.events.pop_front();
        }
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as u64);
        self.events.push_back(Event {
            seq: self.seq,
            at,
            kind,
            subject: subject.into(),
            detail,
        });
    }

    fn after(&self, after: u64, kinds: &[EventKind]) -> Vec<Event> {
        self.events
            .iter()
            .filter(|event| event.seq > after)
            .filter(|event| kinds.is_empty() || kinds.contains(&event.kind))
            .take(BATCH)
            .cloned()
            .collect()
    }
}

fn look(cx: &mut App) -> Seen {
    let mut seen = Seen {
        active: cx.active_window().map(|window| window.window_id().as_u64()),
        ..Seen::default()
    };
    for handle in main_windows(cx) {
        let id = handle.window_id().as_u64();
        seen.windows.push(id);
        if let Ok(hosts) = handle.read_with(cx, |view, _| view.control_hosts()) {
            for host in hosts {
                seen.hosts.insert((id, host.id), host.status);
            }
        }
    }
    seen.windows.sort_unstable();
    if let Some(store) = cx.try_global::<Store>() {
        seen.tabs = store
            .tabs()
            .map(|tab| (u64::from(tab.id), tab.workspace_id.clone()))
            .collect();
    }
    if let Some(feedback) = cx.try_global::<crate::browser::Feedback>() {
        seen.kept = feedback
            .kept()
            .map(|(added, target)| (added, target.pane_id.clone()))
            .collect();
    }
    seen.notes = crate::worktree_notes::Notes::of(cx).map_or(0, |notes| notes.revision());
    #[cfg(feature = "cloud")]
    if let Some(registry) = cx.try_global::<crate::cloud::batch::Registry>() {
        seen.batches = registry.phases().into_iter().collect();
    }
    seen
}

fn compare(log: &mut Log, before: &Seen, now: &Seen) {
    for id in now.windows.iter().filter(|id| !before.windows.contains(id)) {
        log.push(EventKind::WindowOpened, id.to_string(), None);
    }
    for id in before.windows.iter().filter(|id| !now.windows.contains(id)) {
        log.push(EventKind::WindowClosed, id.to_string(), None);
    }
    if now.active != before.active
        && let Some(id) = now.active.filter(|id| now.windows.contains(id))
    {
        log.push(EventKind::WindowFocused, id.to_string(), None);
    }
    for (id, workspace) in &now.tabs {
        if !before.tabs.contains_key(id) {
            log.push(
                EventKind::TabOpened,
                id.to_string(),
                Some(workspace.clone()),
            );
        }
    }
    for (id, workspace) in &before.tabs {
        if !now.tabs.contains_key(id) {
            log.push(
                EventKind::TabClosed,
                id.to_string(),
                Some(workspace.clone()),
            );
        }
    }
    // Every batch kept since the last tick, even when the queue was full
    // and dropped an older one to make room.
    let newest = before.kept.last().map_or(0, |(added, _)| *added);
    for (_, pane) in now.kept.iter().filter(|(added, _)| *added > newest) {
        log.push(EventKind::Feedback, pane.clone(), None);
    }
    if now.notes != before.notes {
        log.push(EventKind::Note, now.notes.to_string(), None);
    }
    for ((window, host), status) in &now.hosts {
        let key = (*window, host.clone());
        if before.hosts.get(&key).is_some_and(|old| old != status) {
            log.push(
                EventKind::Host,
                host.clone(),
                Some(format!("{status} (window {window})")),
            );
        }
    }
    for (id, phase) in &now.batches {
        if before.batches.get(id) != Some(phase) {
            log.push(
                EventKind::CloudBatch,
                id.to_string(),
                Some(phase.name().to_owned()),
            );
        }
    }
}

/// Logs what changed since the last tick.
pub(super) fn record(cx: &mut App) {
    let now = look(cx);
    let mut log = std::mem::take(cx.default_global::<Log>());
    // The first look is the baseline, not a burst of "opened" events.
    if let Some(before) = log.seen.take()
        && before != now
    {
        compare(&mut log, &before, &now);
    }
    log.seen = Some(now);
    *cx.global_mut::<Log>() = log;
}

fn condition_holds(
    condition: Condition,
    caller: &crate::control::protocol::Caller,
    cx: &mut App,
) -> bool {
    match condition {
        Condition::Window => !main_windows(cx).is_empty(),
        Condition::TabClosed { id } => cx
            .try_global::<Store>()
            .is_none_or(|store| store.get(id.into()).is_none()),
        Condition::Online => window_for(caller, cx).is_ok_and(|handle| {
            handle
                .read_with(cx, |view, _| view.live.status.is_connected())
                .unwrap_or(false)
        }),
    }
}

pub(super) fn serve(
    call: Call<EventsOp>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    match call.op {
        EventsOp::Poll {
            after,
            instance,
            wait_seconds,
            kinds,
        } => {
            let now = cx.try_global::<Log>().map_or(0, |log| log.seq);
            let after = start(after, instance, now);
            watches.add(
                incoming,
                Duration::from_secs(wait_seconds),
                move |cx| {
                    let log = cx.try_global::<Log>()?;
                    let events = log.after(after, &kinds);
                    let next = events.last().map_or(after, |event| event.seq);
                    (!events.is_empty()).then_some(Response::Events {
                        reply: Reply::Events {
                            events,
                            next,
                            instance: self::instance(),
                        },
                    })
                },
                Response::Events {
                    reply: Reply::Events {
                        events: Vec::new(),
                        next: after,
                        instance: self::instance(),
                    },
                },
                cx,
            );
        }
        EventsOp::Wait {
            condition,
            wait_seconds,
        } => {
            let caller = call.caller;
            watches.add(
                incoming,
                Duration::from_secs(wait_seconds),
                move |cx| {
                    condition_holds(condition, &caller, cx).then_some(Response::Events {
                        reply: Reply::Waited { met: true },
                    })
                },
                Response::Events {
                    reply: Reply::Waited { met: false },
                },
                cx,
            );
        }
    }
}

#[cfg(test)]
mod tests;
