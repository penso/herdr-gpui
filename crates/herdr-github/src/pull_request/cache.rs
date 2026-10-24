//! One window's PR prefetch: what to ask GitHub and when, over the answers
//! every window shares (see `store`).
//!
//! What was found outlives the connection it was found over: a reconnect,
//! another device, or a renewed token stops the work in flight but keeps the
//! last answers on screen, so a failing or rate-limited refresh never blanks
//! them. Only signing out forgets them.
//!
//! How often a checkout is asked depends on what it last showed: a pull
//! request with checks running every [`REFRESH`], a quiet one or none every
//! [`QUIET_REFRESH`], a merged or closed one every [`SETTLED_REFRESH`]. The
//! workspace being viewed is asked sooner, and an inactive window waits
//! [`IDLE_STRETCH`] times longer.

use super::{
    Input, Lookup, Origin, PullRequest, State,
    batch::{BATCH_LIMIT, Batch},
    lookup::{Finished, Prefetch},
    model::Outcome,
    store::{Entry, Store, View},
};
use chrono::{DateTime, DurationRound as _, Local, TimeDelta};
use std::{
    cell::{Ref, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

pub(super) use super::store::CACHE_LIMIT;
pub(super) const REFRESH: Duration = Duration::from_secs(90);
pub(super) const QUIET_REFRESH: Duration = Duration::from_secs(300);
pub(super) const SETTLED_REFRESH: Duration = Duration::from_secs(1800);
const MISSING_REFRESH: Duration = Duration::from_secs(15);
pub(super) const ERROR_BACKOFF: Duration = Duration::from_secs(300);
/// How long a repository the credential cannot see waits before one branch
/// asks again. Becoming visible takes a user action (installing the GitHub
/// App, a new token), and opening the Git menu asks sooner.
pub(super) const HIDDEN_BACKOFF: Duration = Duration::from_secs(1800);
pub(super) const IDLE_STRETCH: u32 = 5;

/// How many checkouts one request asks for.
pub(super) struct BatchSize(pub(super) usize);

impl Default for BatchSize {
    fn default() -> Self {
        Self(BATCH_LIMIT)
    }
}

/// The scope includes selection epoch, connection generation and daemon boot.
/// Token allocation identity is an additional auth generation, never token text.
#[derive(Default)]
pub struct Cache {
    pub(super) store: Rc<RefCell<Store>>,
    pub(super) prefetch: Prefetch,
    pub(super) queue: std::collections::VecDeque<Input>,
    /// The checkouts of the batch in flight.
    pub(super) active: Vec<Input>,
    pub(super) scope: Option<(u64, u64, String)>,
    pub(super) token: Option<crate::Tokens>,
    pub(super) view: View,
    pub(super) batch: BatchSize,
    pub(super) next_scan: Option<Instant>,
    /// No window of this cache is in front, so routine refreshes wait longer.
    pub idle: bool,
    /// The store's revision this window last showed.
    pub(super) seen: u64,
    pub cursor: usize,
}

impl Drop for Cache {
    fn drop(&mut self) {
        self.release_active();
    }
}

/// When an entry is next due: its own time, stretched for an idle window
/// unless someone asked for it.
fn due(entry: &Entry, idle: bool) -> Instant {
    if !idle || entry.urgent {
        return entry.due;
    }
    entry.checked + entry.due.saturating_duration_since(entry.checked) * IDLE_STRETCH
}

/// How long an answer stays fresh: the less a pull request can still change,
/// the less often it is asked about.
fn freshness(value: Option<&PullRequest>) -> Duration {
    match value {
        Some(pr) if matches!(pr.state, State::Merged | State::Closed) => SETTLED_REFRESH,
        Some(pr)
            if pr
                .check_list()
                .any(|(_, outcome)| outcome == Outcome::Pending) =>
        {
            REFRESH
        }
        _ => QUIET_REFRESH,
    }
}

impl Cache {
    /// A cache over the store the app's windows share.
    pub fn shared(store: Rc<RefCell<Store>>) -> Self {
        let mut cache = Self::default();
        cache.store = store;
        cache
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn seed(&mut self, input: Input, value: PullRequest, now: Instant) {
        let mut store = self.store.borrow_mut();
        store
            .entries
            .retain(|entry| entry.view != self.view || entry.input != input);
        store.record(
            &self.view,
            input,
            Some(Some(value)),
            None,
            (now, now + REFRESH),
        );
    }

    /// Forgets this account set's answers and its accounts' pauses, for
    /// signing out.
    pub fn clear(&mut self) {
        self.suspend();
        if let Ok(mut store) = self.store.try_borrow_mut() {
            let logins = self.token.iter().flat_map(crate::Tokens::logins);
            store.forget(&self.view.login, logins);
        }
        self.scope = None;
        self.token = None;
        self.view = View::default();
        self.cursor = 0;
    }

    /// Stops the batch in flight and the queued lookups but keeps what was
    /// found and any pause, for a connection that dropped or changed. A late
    /// answer to the stopped batch is discarded by its generation.
    pub fn suspend(&mut self) {
        self.prefetch.clear();
        self.queue.clear();
        self.release_active();
        self.next_scan = None;
    }

    fn release_active(&mut self) {
        let active = std::mem::take(&mut self.active);
        if let Ok(mut store) = self.store.try_borrow_mut() {
            store.release(&self.view, &active);
        }
    }

    /// A different account, device, connection, daemon boot, or renewed token
    /// stops the work in flight and keeps what was found, each entry under
    /// the device and account it belongs to.
    pub fn scope(
        &mut self,
        scope: (u64, u64, String),
        token: impl Into<crate::Tokens>,
        login: &str,
        origin: Origin,
    ) {
        let token = token.into();
        let changed = self.scope.as_ref() != Some(&scope)
            || self.view.origin != origin
            || self.view.login != login
            || self.token.as_ref().is_none_or(|old| !old.same(&token));
        if changed {
            self.suspend();
            self.scope = Some(scope);
            self.token = Some(token);
            self.view = View {
                origin,
                login: login.to_owned(),
            };
        }
    }

    /// Drops this device's checkouts that are gone. Other devices' entries
    /// wait to be seen again, bounded by [`CACHE_LIMIT`] with the rest.
    pub fn retain(&mut self, current: impl Fn(&Input) -> bool) {
        self.queue.retain(&current);
        let (kept, gone) = std::mem::take(&mut self.active)
            .into_iter()
            .partition(|input| current(input));
        // The rest of the batch is still answered; a gone checkout's answer
        // is ignored when it arrives.
        self.active = kept;
        let Ok(mut store) = self.store.try_borrow_mut() else {
            return;
        };
        store.release(&self.view, &gone);
        let view = &self.view;
        store
            .entries
            .retain(|entry| &entry.view != view || current(&entry.input));
    }

    pub fn scan_due(&self, now: Instant) -> bool {
        self.next_scan.is_none_or(|next| now >= next)
    }

    /// The workspace being viewed is asked sooner than the rest: a pull
    /// request may have been opened for an empty one, and an open one's
    /// reviews move while it is looked at. Errors keep their backoff and a
    /// merged or closed pull request its own. Only the background poll calls
    /// this, never rendering, and it does not change queue fairness.
    pub fn expire_viewed(&mut self, input: &Input, now: Instant) {
        let Ok(mut store) = self.store.try_borrow_mut() else {
            return;
        };
        let Some(entry) = store.entry_mut(&self.view, input) else {
            return;
        };
        let wait = match &entry.value {
            _ if entry.message.is_some() => return,
            None => MISSING_REFRESH,
            Some(pr) if pr.state == State::Open => REFRESH,
            Some(_) => return,
        };
        // Not urgent: an inactive window stretches this wait like any other.
        if now >= entry.checked + wait {
            entry.due = entry.due.min(now);
        }
    }

    /// Request fresh details without discarding the visible value or interrupting
    /// an in-flight lookup. The poll task still owns dispatch and account backoff.
    pub fn refresh(&mut self, input: Input, now: Instant) {
        if self.active.contains(&input) {
            return;
        }
        if let Ok(mut store) = self.store.try_borrow_mut() {
            if let Some(entry) = store.entry_mut(&self.view, &input) {
                entry.due = now;
                entry.urgent = true;
            }
            // Asking explicitly is how a newly installed App is noticed.
            if let Some(key) = &input.repo_key {
                store.unhide(&self.view, key);
            }
        }
        self.queue.retain(|queued| queued != &input);
        self.queue.truncate(CACHE_LIMIT - 1);
        self.queue.push_front(input);
        // Let the next poll dispatch this priority request before a routine scan
        // replaces the queue with the snapshot's round-robin order.
        self.next_scan = Some(now + Duration::from_secs(1));
    }

    pub fn schedule(&mut self, inputs: impl IntoIterator<Item = Input>, now: Instant) {
        self.next_scan = Some(now + Duration::from_secs(1));
        // Replace queued metadata, not an unbounded history of snapshot changes.
        self.queue.clear();
        let Ok(store) = self.store.try_borrow() else {
            return;
        };
        for input in inputs {
            if self.queue.len() == CACHE_LIMIT {
                break;
            }
            if !self.active.contains(&input)
                && !store.asking(&self.view, &input)
                && !self.queue.contains(&input)
                && store
                    .entry(&self.view, &input)
                    .is_none_or(|entry| now >= due(entry, self.idle))
            {
                self.queue.push_back(input);
            }
        }
    }

    /// Collects and dispatches lookups. Whether anything this window shows
    /// changed, including answers another window recorded.
    pub fn poll(&mut self, now: Instant) -> bool {
        let changed = self.dispatch(now);
        let Ok(store) = self.store.try_borrow() else {
            return changed;
        };
        let shown = std::mem::replace(&mut self.seen, store.revision);
        changed || shown != store.revision
    }

    fn dispatch(&mut self, now: Instant) -> bool {
        let mut changed = self.collect(now);
        let Some(token) = self.token.clone().filter(|_| self.active.is_empty()) else {
            return changed;
        };
        let Ok(mut store) = self.store.try_borrow_mut() else {
            return changed;
        };
        // A paused account's checkouts are held by the worker, which alone
        // learns whose they are; with every account paused there is nothing
        // to ask.
        if pause(&store, &token, now).is_some() {
            return changed;
        }
        let paused: Vec<String> = token
            .logins()
            .filter(|login| store.pause(login, now).is_some())
            .map(str::to_owned)
            .collect();
        let mut batch = Vec::new();
        while batch.len() < self.batch.0.max(1)
            && let Some(input) = self.queue.pop_front()
        {
            // A hidden repository's branches wait with it, without asking.
            if let Some((until, message)) = store.hidden_for(&self.view, &input, now) {
                store.record(&self.view, input, None, Some(message), (now, until));
                changed = true;
                continue;
            }
            let existing = store.entry(&self.view, &input);
            if existing.is_some_and(|entry| now < due(entry, self.idle))
                || (existing.is_none()
                    && store.entries.len() == CACHE_LIMIT
                    && store.entries.iter().all(|entry| now < entry.due))
                || store.asking(&self.view, &input)
                || batch.contains(&input)
            {
                continue;
            }
            batch.push(input);
        }
        if batch.is_empty() {
            return changed;
        }
        store
            .asking
            .extend(batch.iter().map(|input| (self.view.clone(), input.clone())));
        store.changed();
        drop(store);
        self.cursor = self.cursor.wrapping_add(batch.len());
        self.prefetch
            .request(batch.clone(), self.view.origin.clone(), token, paused);
        self.active = batch;
        // Send it now; only a worker that cannot start answers here.
        self.collect(now);
        true
    }

    /// Records the batch in flight once it is answered.
    fn collect(&mut self, now: Instant) -> bool {
        // Borrowed before the answer is taken, so it is never taken and lost.
        let Ok(mut store) = self.store.try_borrow_mut() else {
            return false;
        };
        let Some(finished) = self.prefetch.poll() else {
            return false;
        };
        let active = std::mem::take(&mut self.active);
        store.release(&self.view, &active);
        let (found, failed) = match finished {
            Finished::Answered(batch) => {
                // Rate/auth failures pause the account that hit them, never
                // the others; a bad local repo must not prevent the remaining
                // workspaces from being prefetched.
                for paused in &batch.paused {
                    let reason = batch
                        .failed
                        .iter()
                        .find(|(inputs, _)| {
                            inputs.iter().any(|input| paused.inputs.contains(input))
                        })
                        .map(|(_, error)| error.to_string())
                        .or_else(|| {
                            batch.found.iter().find_map(|(input, result)| {
                                result
                                    .as_ref()
                                    .err()
                                    .filter(|_| paused.inputs.contains(input))
                                    .map(ToString::to_string)
                            })
                        });
                    store.set_pause(&paused.login, now + paused.cooldown, reason);
                }
                // A paused account's checkouts wait for it, saying why.
                for (input, login) in batch.held {
                    if !active.contains(&input) {
                        continue;
                    }
                    if let Some((until, reason)) = store.pause(&login, now) {
                        let message = paused(until - now, reason, Local::now());
                        store.hold(&self.view, input, message, (now, until));
                    }
                }
                // Checkouts the batch had no time to prepare stay due and are
                // asked in a later one.
                let Batch { found, failed, .. } = batch;
                let failed = failed
                    .into_iter()
                    .map(|(inputs, error)| (inputs, error.to_string()))
                    .collect();
                (found, failed)
            }
            Finished::Failed(message) => (Vec::new(), vec![(active.clone(), message.to_owned())]),
        };
        for (input, result) in found {
            if !active.contains(&input) {
                continue;
            }
            match result {
                Ok(value) => {
                    let due = now + freshness(value.as_ref());
                    store.record(&self.view, input, Some(value), None, (now, due));
                }
                Err(error) => {
                    let message = error.to_string();
                    if let crate::Error::RepositoryHidden { repository, .. } = error
                        && let Some(repo_key) = input.repo_key.clone()
                    {
                        let until = now + HIDDEN_BACKOFF;
                        store.hide(&self.view, (repository, repo_key), message.clone(), until);
                        store.record(&self.view, input, None, Some(message), (now, until));
                    } else {
                        let due = now + ERROR_BACKOFF;
                        store.record(&self.view, input, None, Some(message), (now, due));
                    }
                }
            }
        }
        for (inputs, message) in failed {
            for input in inputs.into_iter().filter(|input| active.contains(input)) {
                let due = now + ERROR_BACKOFF;
                store.record(&self.view, input, None, Some(message.clone()), (now, due));
            }
        }
        true
    }

    /// Whether a lookup for `input` is in flight or queued to dispatch, so the
    /// chrome can say it is waiting on GitHub. A paused account is not loading.
    pub fn loading(&self, input: &Input, now: Instant) -> bool {
        let Ok(store) = self.store.try_borrow() else {
            return false;
        };
        self.active.contains(input)
            || store.asking(&self.view, input)
            || (self
                .token
                .as_ref()
                .is_some_and(|token| pause(&store, token, now).is_none())
                && self.queue.contains(input))
    }

    /// Pure cache read for chrome painted every frame: never schedules work and
    /// never reorders the cache, so rendering cannot start a request. `view`
    /// is the device the row's checkout is on and its account set, which need
    /// not be the selected device's: each device's answers are kept apart, so
    /// the same path and branch on two hosts never share one.
    pub fn peek(&self, view: &View, repo_key: &str, branch: &str) -> Option<Ref<'_, PullRequest>> {
        let store = self.store.try_borrow().ok()?;
        Ref::filter_map(store, |store| {
            store
                .entries
                .iter()
                .find(|entry| {
                    entry.view == *view
                        && entry.input.checkout.is_none()
                        && entry.input.repo_key.as_deref() == Some(repo_key)
                        && entry.input.branch == branch
                })
                .and_then(|entry| entry.value.as_ref())
        })
        .ok()
    }

    /// Pure cache read of exactly `input`'s pull request, as `peek` is for a
    /// row named by daemon metadata. A local space without that metadata is
    /// looked up by its directory, which only its own input matches.
    pub fn peek_input(&self, view: &View, input: &Input) -> Option<Ref<'_, PullRequest>> {
        let store = self.store.try_borrow().ok()?;
        Ref::filter_map(store, |store| {
            store
                .entry(view, input)
                .and_then(|entry| entry.value.as_ref())
        })
        .ok()
    }

    /// Pure cache read of one checkout's lookup, local checkouts included:
    /// `None` until it was looked up once, else its pull request (if any)
    /// and why its latest refresh failed (if it did).
    pub fn lookup(&self, input: &Input) -> Option<(Option<PullRequest>, Option<String>)> {
        let store = self.store.try_borrow().ok()?;
        store
            .entry(&self.view, input)
            .map(|entry| (entry.value.clone(), entry.message.clone()))
    }

    /// Pure cache read by lookup key, for the merged worktree cleanup.
    pub fn get(&self, input: &Input) -> Option<Ref<'_, PullRequest>> {
        let store = self.store.try_borrow().ok()?;
        Ref::filter_map(store, |store| {
            store
                .entry(&self.view, input)
                .and_then(|entry| entry.value.as_ref())
        })
        .ok()
    }

    /// Pure cache read: opening a menu cannot launch Git, HTTPS or daemon requests.
    pub fn present(&mut self, input: &Input, view: &mut Lookup, now: Instant) {
        view.clear();
        let Ok(mut store) = self.store.try_borrow_mut() else {
            view.loading = true;
            return;
        };
        if let Some(entry) = store.entry_mut(&self.view, input) {
            entry.used = now;
            view.value = entry.value.clone();
            view.message = entry.message.clone();
            view.checked = Some(entry.checked);
        } else if let Some((until, reason)) = self
            .token
            .as_ref()
            .and_then(|token| pause(&store, token, now))
        {
            view.message = Some(paused(until - now, reason, Local::now()));
        } else {
            view.loading = true;
        }
        // A stale value stays visible, but the reason it is stale must too.
        if view.value.is_some()
            && let Some(message) = view.message.take()
        {
            view.message = Some(format!(
                "Refresh unavailable, showing the last result. {message}"
            ));
        }
    }
}

/// While every account of `tokens` is paused, when the first resumes and why
/// it stopped. One healthy account keeps the set asking.
fn pause<'a>(
    store: &'a Store,
    tokens: &crate::Tokens,
    now: Instant,
) -> Option<(Instant, Option<&'a str>)> {
    tokens
        .logins()
        .map(|login| store.pause(login, now))
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .min_by_key(|(until, _)| *until)
}

/// Why GitHub requests are paused and the local minute they resume, rounded
/// up so the shown time is never early.
pub(super) fn paused(wait: Duration, reason: Option<&str>, now: DateTime<Local>) -> String {
    let resume = TimeDelta::from_std(wait)
        .ok()
        .and_then(|wait| now.checked_add_signed(wait))
        .map(|at| at.duration_round_up(TimeDelta::minutes(1)).unwrap_or(at));
    let when = resume.map_or("later".to_owned(), |at| {
        format!("at {}", at.format("%H:%M"))
    });
    match reason {
        Some(reason) => format!("{reason} GitHub requests paused; retrying {when}."),
        None => format!(
            "GitHub requests paused after an authentication or rate-limit error; retrying {when}."
        ),
    }
}
