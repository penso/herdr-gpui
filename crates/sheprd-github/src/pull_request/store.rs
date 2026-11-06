//! What every window's PR cache shares: the answers, each account's pause,
//! the repositories GitHub would not show, and the lookups some window has in
//! flight. Two windows showing the same checkouts then ask GitHub once, and a
//! rate limit one window hit pauses the others. Bounded like a single cache.

use super::{Input, Origin, PullRequest};
use std::time::Instant;

pub(super) const CACHE_LIMIT: usize = 128;

/// Whose lookups an entry is: the device the checkout is on and the GitHub
/// account that read it. A window refreshes only its selected device's view,
/// and each device's rows show their own view's answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub origin: Origin,
    /// The account set's key, as [`crate::Tokens::key`] gives it.
    pub login: String,
}

pub(super) struct Entry {
    pub(super) view: View,
    pub(super) input: Input,
    pub(super) value: Option<PullRequest>,
    pub(super) message: Option<String>,
    pub(super) checked: Instant,
    pub(super) due: Instant,
    pub(super) used: Instant,
    /// Asked for explicitly, so an idle window does not stretch its wait.
    pub(super) urgent: bool,
}

/// A repository GitHub would not show this credential, paused as a whole so
/// its branches do not each ask on their own backoff.
pub(super) struct HiddenRepository {
    pub(super) view: View,
    pub(super) repo_key: String,
    pub(super) until: Instant,
    pub(super) message: String,
}

/// An account GitHub refused, until when, and the failure that paused it.
pub(super) struct Pause {
    pub(super) login: String,
    pub(super) until: Instant,
    pub(super) reason: Option<String>,
    pub(super) rate_limited: bool,
}

#[derive(Default)]
pub struct Store {
    pub(super) entries: Vec<Entry>,
    pub(super) hidden: Vec<HiddenRepository>,
    pub(super) pauses: Vec<Pause>,
    /// Checkouts some window is asking about, so another does not ask too.
    pub(super) asking: Vec<(View, Input)>,
    /// Counts every change a window could show, so each window redraws
    /// when another one records an answer, a pause, or a lookup.
    pub(super) revision: u64,
}

impl Store {
    pub(super) fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(super) fn entry(&self, view: &View, input: &Input) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|entry| &entry.view == view && &entry.input == input)
    }

    pub(super) fn entry_mut(&mut self, view: &View, input: &Input) -> Option<&mut Entry> {
        self.entries
            .iter_mut()
            .find(|entry| &entry.view == view && &entry.input == input)
    }

    pub(super) fn asking(&self, view: &View, input: &Input) -> bool {
        self.asking
            .iter()
            .any(|(asked, asking)| asked == view && asking == input)
    }

    pub(super) fn release(&mut self, view: &View, inputs: &[Input]) {
        let before = self.asking.len();
        self.asking
            .retain(|(asked, input)| asked != view || !inputs.contains(input));
        // Suspending runs every tick while disconnected: only a real change
        // redraws the windows.
        if self.asking.len() != before {
            self.changed();
        }
    }

    /// When `login`'s requests resume and why they stopped, while paused.
    pub(super) fn pause(&self, login: &str, now: Instant) -> Option<(Instant, Option<&str>)> {
        self.pauses
            .iter()
            .find(|pause| pause.login == login && now < pause.until)
            .map(|pause| (pause.until, pause.reason.as_deref()))
    }

    pub(super) fn set_pause(&mut self, login: &str, until: Instant, error: Option<&crate::Error>) {
        self.changed();
        self.pauses.retain(|pause| pause.login != login);
        if self.pauses.len() == CACHE_LIMIT {
            self.pauses.remove(0);
        }
        self.pauses.push(Pause {
            login: login.to_owned(),
            until,
            reason: error.map(ToString::to_string),
            rate_limited: matches!(error, Some(crate::Error::RateLimit { .. })),
        });
    }

    /// Forgets an account set's answers and its accounts' pauses, for
    /// signing it out.
    pub(super) fn forget<'a>(&mut self, login: &str, accounts: impl IntoIterator<Item = &'a str>) {
        let accounts: Vec<&str> = accounts.into_iter().collect();
        let before = (
            self.entries.len(),
            self.hidden.len(),
            self.pauses.len(),
            self.asking.len(),
        );
        self.entries.retain(|entry| entry.view.login != login);
        self.hidden.retain(|hidden| hidden.view.login != login);
        self.pauses
            .retain(|pause| !accounts.contains(&pause.login.as_str()));
        self.asking.retain(|(view, _)| view.login != login);
        // Clearing runs every tick while signed out, like `release`.
        let after = (
            self.entries.len(),
            self.hidden.len(),
            self.pauses.len(),
            self.asking.len(),
        );
        if after != before {
            self.changed();
        }
    }

    /// Stores a finished (or skipped) lookup. `value` is `None` to keep the
    /// visible one, as a failure does. A new entry in a full cache evicts the
    /// least recently used one that is due, else the least recently used, so
    /// the store never outgrows [`CACHE_LIMIT`].
    pub(super) fn record(
        &mut self,
        view: &View,
        input: Input,
        value: Option<Option<PullRequest>>,
        message: Option<String>,
        (now, due): (Instant, Instant),
    ) {
        self.changed();
        if let Some(entry) = self.entry_mut(view, &input) {
            if let Some(value) = value {
                entry.value = value;
            }
            entry.message = message;
            entry.checked = now;
            entry.due = due;
            entry.urgent = false;
            return;
        }
        if self.entries.len() >= CACHE_LIMIT {
            let least_used = |due_only: bool| {
                self.entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| !due_only || now >= e.due)
                    .min_by_key(|(_, e)| e.used)
                    .map(|(index, _)| index)
            };
            if let Some(index) = least_used(true).or_else(|| least_used(false)) {
                self.entries.remove(index);
            }
        }
        self.entries.push(Entry {
            view: view.clone(),
            input,
            value: value.flatten(),
            message,
            checked: now,
            due,
            used: now,
            urgent: false,
        });
    }

    /// Holds a checkout whose account is paused until `until`, saying why,
    /// without marking it checked: its visible answer stays as it was.
    pub(super) fn hold(
        &mut self,
        view: &View,
        input: Input,
        message: String,
        (now, until): (Instant, Instant),
    ) {
        match self.entry_mut(view, &input) {
            Some(entry) => {
                entry.message = Some(message);
                entry.due = entry.due.max(until);
                entry.urgent = false;
                self.changed();
            }
            None => self.record(view, input, None, Some(message), (now, until)),
        }
    }

    /// Pauses every branch of `repo_key` until `until`, and shows why on those
    /// already looked up, so none asks GitHub again on its own backoff.
    pub(super) fn hide(
        &mut self,
        view: &View,
        (repository, repo_key): (String, String),
        message: String,
        until: Instant,
    ) {
        self.changed();
        tracing::info!(
            category = "github_pause",
            repository = repository.as_str(),
            seconds = until.saturating_duration_since(Instant::now()).as_secs(),
            "Pausing pull request lookups for a repository GitHub would not show"
        );
        for entry in &mut self.entries {
            if &entry.view == view && entry.input.repo_key.as_deref() == Some(repo_key.as_str()) {
                entry.message = Some(message.clone());
                entry.due = entry.due.max(until);
            }
        }
        self.hidden
            .retain(|hidden| &hidden.view != view || hidden.repo_key != repo_key);
        if self.hidden.len() == CACHE_LIMIT {
            self.hidden.remove(0);
        }
        self.hidden.push(HiddenRepository {
            view: view.clone(),
            repo_key,
            until,
            message,
        });
    }

    /// When `input`'s repository resumes and why it is paused, while it is.
    pub(super) fn hidden_for(
        &mut self,
        view: &View,
        input: &Input,
        now: Instant,
    ) -> Option<(Instant, String)> {
        self.hidden.retain(|hidden| now < hidden.until);
        let key = input.repo_key.as_deref()?;
        self.hidden
            .iter()
            .find(|hidden| &hidden.view == view && hidden.repo_key == key)
            .map(|hidden| (hidden.until, hidden.message.clone()))
    }

    /// Lifts the pause on `repo_key`, for asking about one of its branches.
    pub(super) fn unhide(&mut self, view: &View, repo_key: &str) {
        self.hidden
            .retain(|hidden| &hidden.view != view || hidden.repo_key != repo_key);
    }
}
