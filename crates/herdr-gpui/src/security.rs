//! The activity bar's Security section: the private vulnerability reports
//! outsiders filed (`reports`) and the open Dependabot alerts of every
//! github.com repository the window's workspaces hold, most severe first.
//! Its icon is always in the bar, so it never waits on the first read; a
//! search field and pills (`filter`) narrow what the section lists.
//!
//! The repositories are the ones the Pull requests and Issues sections list
//! (`HerdrWindow::repo_specs`), read with the same GitHub accounts. A
//! background read runs every [`INTERVAL`], or when Refresh asks; drawing
//! reads what it left. The app's cache keeps the last read (`saved`), shown
//! at start until the first read of the run replaces it.

mod actions;
mod fetch;
mod filter;
mod list;
mod model;
mod page;
mod pane;
mod reports;
mod saved;
mod section;

#[cfg(test)]
mod tests;

pub(crate) use filter::SecuritySearch;
pub(crate) use model::{Alert, Alerts, Repo, Severity};
pub(crate) use page::{Item, Tab};
pub(crate) use reports::{Report, Reports};

use crate::{HerdrWindow, browser::WebUrl, sidebar::ActivityPanel};
use gpui::{prelude::*, *};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

/// How often the alerts are read again. They change when a dependency is
/// bumped or an advisory is published, not from minute to minute.
const INTERVAL: Duration = Duration::from_secs(15 * 60);
/// How soon to look again while there is nothing to read: no GitHub account
/// yet, or no workspace holding a repository.
const WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Github(Box<crate::Error>),
    #[error("GitHub's answer had no repository in it")]
    Output,
    #[error("Cannot read GitHub's alerts listing: {0}")]
    Json(#[from] serde_json::Error),
    /// Restored from the cache, which held no read of it yet.
    #[error("Not read yet")]
    Unread,
}

impl Error {
    /// Whether GitHub refused for the account's spent quota.
    fn rate_limited(&self) -> bool {
        matches!(self, Self::Github(error) if matches!(**error, crate::Error::GitHub(herdr_github::Error::RateLimit { .. })))
    }
}

/// The section's state in a window.
pub(crate) struct Security {
    repos: Vec<Repo>,
    /// When the last read finished, for "Updated … ago".
    read_at: Option<SystemTime>,
    /// When the next read is due; `None` reads at the next tick.
    next: Option<Instant>,
    task: Option<Task<()>>,
    /// The GitHub logins `repos` was read as. Private reports are only for
    /// the accounts that may see them, so what one sign-in read is dropped
    /// once the window reads GitHub as anyone else.
    accounts: Vec<String>,
    /// Tells a read in flight to stop when the window goes.
    cancelled: Arc<AtomicBool>,
    scroll: ScrollHandle,
    /// The alert or report whose page is shown beside the list.
    opened: Option<page::Opened>,
    /// Whether the page takes the whole section, the list folded away.
    list_hidden: bool,
    /// The page's width beside the list, as its handle was dragged.
    pane_width: f32,
    /// Pages read this session, by item.
    pages: HashMap<Item, page::Loaded>,
    page_scroll: ScrollHandle,
    /// The app's cache file; tests keep none unless they set one.
    cache: Option<PathBuf>,
    /// Reads the cached read for the accounts just signed in.
    restoring: Option<Task<()>>,
}

impl Default for Security {
    fn default() -> Self {
        Self {
            repos: Vec::new(),
            read_at: None,
            next: None,
            task: None,
            accounts: Vec::new(),
            cancelled: Arc::default(),
            scroll: ScrollHandle::new(),
            opened: None,
            list_hidden: false,
            pane_width: crate::orchestrator::pane::PANE_DEFAULT,
            pages: HashMap::new(),
            page_scroll: ScrollHandle::new(),
            cache: if cfg!(test) {
                None
            } else {
                crate::cache::path()
                    .inspect_err(|error| tracing::debug!(%error, "No cache for Security"))
                    .ok()
            },
            restoring: None,
        }
    }
}

impl Drop for Security {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Security {
    /// The repositories as last read.
    pub(crate) fn repos(&self) -> &[Repo] {
        &self.repos
    }

    pub(crate) fn read_at(&self) -> Option<SystemTime> {
        self.read_at
    }

    pub(crate) fn busy(&self) -> bool {
        self.task.is_some()
    }

    /// Open critical and high alerts, and every report in triage whatever
    /// its rating, since each waits on a reply: the bar's count.
    pub(crate) fn urgent(&self) -> usize {
        let alerts = self
            .repos
            .iter()
            .flat_map(Repo::open)
            .filter(|alert| alert.severity.urgent())
            .count();
        alerts
            + self
                .repos
                .iter()
                .map(|repo| repo.reported().len())
                .sum::<usize>()
    }

    /// When the newest of what [`Self::urgent`] counts was opened, so the
    /// bar can tell whether it arrived since the section was last open.
    pub(crate) fn urgent_since(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        let alerts = self
            .repos
            .iter()
            .flat_map(Repo::open)
            .filter(|alert| alert.severity.urgent())
            .filter_map(|alert| alert.created_at);
        let reports = self
            .repos
            .iter()
            .flat_map(Repo::reported)
            .filter_map(|report| report.created_at);
        alerts.chain(reports).max()
    }

    /// Installs `repos` as a read would, for tests outside this module.
    #[cfg(test)]
    pub(crate) fn read_for_test(&mut self, repos: Vec<Repo>) {
        self.install(repos, Instant::now(), SystemTime::now());
    }

    /// Reads again at the next tick.
    pub(crate) fn refresh(&mut self) {
        self.next = None;
    }

    fn due(&self, now: Instant) -> bool {
        self.task.is_none() && self.next.is_none_or(|next| now >= next)
    }

    /// Drops what was read, and the answer of a read in flight with it:
    /// dropping its task drops the update that would install it.
    fn forget(&mut self) {
        self.repos.clear();
        self.read_at = None;
        self.next = None;
        self.task = None;
        self.restoring = None;
        self.opened = None;
        self.pages.clear();
    }

    /// What the cache kept, read at `at`, unless this run's own read is in.
    fn restore(&mut self, repos: Vec<Repo>, at: SystemTime) {
        if self.read_at.is_some() {
            return;
        }
        self.repos = repos;
        self.keep_opened();
        self.read_at = Some(at);
    }

    /// What a read found, replacing the last one.
    fn install(&mut self, repos: Vec<Repo>, now: Instant, at: SystemTime) {
        self.repos = repos;
        self.keep_opened();
        self.read_at = Some(at);
        self.next = Some(now + INTERVAL);
        self.task = None;
    }
}

impl HerdrWindow {
    /// Runs every tick: starts a read when one is due and there is a
    /// repository to read with an account to read it.
    pub(crate) fn poll_security(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let accounts = self.github_logins();
        if accounts != self.security.accounts {
            self.security.forget();
            self.security.accounts = accounts;
            cx.notify();
        }
        if !self.activity_bar_shown() || !self.security.due(now) {
            return;
        }
        let specs: Vec<_> = self
            .repo_specs()
            .into_iter()
            .filter(|spec| spec.request.token.is_some())
            .collect();
        if specs.is_empty() {
            // Nothing left to read: the last read's repositories are no
            // longer open, or no longer readable with any sign-in.
            if !self.security.repos.is_empty() {
                self.security.install(Vec::new(), now, SystemTime::now());
                cx.notify();
            }
            self.security.next = Some(now + WAIT);
            return;
        }
        // What this window reads, as the cache keeps it: another window
        // signed in as the same accounts may hold other repositories.
        let key = saved::key(&self.security.accounts, &specs);
        if self.security.read_at.is_none() {
            self.restore_security(key.clone(), cx);
        }
        let cancelled = self.security.cancelled.clone();
        let cache = self.security.cache.clone();
        let work = cx.background_executor().spawn(async move {
            let repos = fetch::read(&specs, &cancelled);
            // A cancelled read stopped short: it is no read to keep.
            if !cancelled.load(Ordering::Relaxed)
                && let Some(cache) = cache.and_then(|path| open_cache(&path))
            {
                saved::save(&cache, &key, &repos);
            }
            repos
        });
        self.security.task = Some(cx.spawn(async move |this, cx| {
            let repos = work.await;
            let _ = this.update(cx, |this, cx| {
                this.security
                    .install(repos, Instant::now(), SystemTime::now());
                this.read_waiting_page(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Lists what the cache kept under `key` for this window, in the
    /// background, until the first read of the run is in.
    fn restore_security(&mut self, key: String, cx: &mut Context<Self>) {
        let Some(path) = self.security.cache.clone() else {
            return;
        };
        let work = cx
            .background_executor()
            .spawn(async move { saved::restore(&open_cache(&path)?, &key) });
        self.security.restoring = Some(cx.spawn(async move |this, cx| {
            let restored = work.await;
            let _ = this.update(cx, |this, cx| {
                this.security.restoring = None;
                if let Some((repos, at)) = restored {
                    this.security.restore(repos, at);
                    cx.notify();
                }
            });
        }));
    }

    /// Every login the window reads GitHub as, on this device and on its
    /// hosts, sorted.
    fn github_logins(&self) -> Vec<String> {
        let mut logins: Vec<String> = std::iter::once(&self.menu.github)
            .chain(self.menu.github_hosts.values())
            .chain(
                self.menu
                    .github_accounts
                    .iter()
                    .map(|account| &account.auth),
            )
            .filter_map(|auth| auth.profile.as_ref())
            .map(|profile| profile.login.clone())
            .collect();
        logins.sort_unstable();
        logins.dedup();
        logins
    }

    /// Shows `url`, an alert's or repository's page, in a browser tab of the
    /// focused workspace, and the terminals behind it; Back returns here.
    fn open_security_page(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(url) = WebUrl::try_from(url) else {
            return;
        };
        let Some((_, workspace)) = self.browser_key() else {
            self.show_flash(crate::window::Flash::warning("Open a workspace first"), cx);
            return;
        };
        let endpoint = self.endpoints[self.selected_endpoint].id.clone();
        let opened = self.open_workspace_page(&endpoint, &workspace, url, window, cx);
        // The system browser leaves the section shown; a tab is behind it.
        if opened == crate::browser::PageOpened::Tab {
            self.activity.pick(ActivityPanel::Workspaces);
            self.activity.return_to = Some(ActivityPanel::Security);
            self.save_chrome();
        }
        cx.notify();
    }
}

fn open_cache(path: &std::path::Path) -> Option<crate::cache::Cache> {
    crate::cache::Cache::open_at(path)
        .inspect_err(|error| tracing::debug!(%error, "Cannot open the cache"))
        .ok()
}
