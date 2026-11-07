//! The single worker behind the pull request and issue tabs. One request is in
//! flight at a time and a superseded one is dropped by generation, so a reply
//! from a dismissed dialog can never land in a newer one.

use super::{Item, Origin, Target, fetch};
use crate::github::Tokens;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

/// The accounts a listing may read with: GitHub's, when one is connected,
/// and where saved GitLab tokens are kept.
#[derive(Clone)]
pub(crate) struct Accounts {
    pub github: Option<Tokens>,
    pub gitlab: sheprd_credentials::Store,
}

/// What the worker was asked to do. Listing and preparing are both bounded,
/// Git and GitHub work; fetching updates refs but never checkout files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Task {
    List,
    /// Make the PR head reachable before a checkout is asked for.
    Fetch(Item),
}

/// What the worker finished. `Fetched` carries the branch back so a stale reply
/// cannot arm a checkout for a different pull request.
#[derive(Debug)]
pub(crate) enum Done {
    Listed(Origin, Vec<Item>),
    /// A GitHub repository with no GitHub account: nothing to list, and
    /// nothing to report, since its tabs say where to sign in.
    Unlisted,
    Fetched(String),
}

/// The forge a listing found, which outlives a failed listing, and what the
/// worker finished.
type Answer = (Option<super::Forge>, crate::Result<Done>);

struct Worker {
    requests: mpsc::SyncSender<(u64, Target, Accounts, Task)>,
    results: mpsc::Receiver<(u64, Answer)>,
}

#[derive(Default)]
pub(crate) struct Lookup {
    worker: Option<Worker>,
    generation: Arc<AtomicU64>,
    busy: bool,
    waiting: Option<(Target, Accounts, Task)>,
    /// Set while any request is outstanding, so the tabs can say so.
    pub loading: bool,
    pub origin: Option<Origin>,
    pub items: Vec<Item>,
    pub message: Option<String>,
    /// The branch a finished fetch made available, for the window to act on.
    pub ready: Option<String>,
    /// Where the repository lives, once a listing found its remote.
    forge: Option<super::Forge>,
    /// The listing runs without a GitHub account, only to find a GitLab
    /// project: its failures are logged, not shown, unless it found one.
    quiet: bool,
    cooldown: Option<Duration>,
}

impl Drop for Lookup {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
}

impl Lookup {
    pub fn listed(&self) -> bool {
        self.origin.is_some()
    }

    pub fn list(&mut self, input: Target, accounts: Accounts) {
        self.quiet = accounts.github.is_none();
        self.request(input, accounts, Task::List);
        self.items.clear();
        self.origin = None;
    }

    pub fn fetch_branch(&mut self, input: Target, accounts: Accounts, item: &Item) {
        self.request(input, accounts, Task::Fetch(item.clone()));
    }

    /// Where the repository lives, once a listing found its remote.
    pub fn forge(&self) -> Option<&super::Forge> {
        self.forge.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn install_forge(&mut self, forge: super::Forge) {
        self.forge = Some(forge);
    }

    fn request(&mut self, input: Target, token: Accounts, task: Task) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.waiting = Some((input, token, task));
        self.loading = true;
        self.message = None;
        self.ready = None;
    }

    /// Drain the worker and schedule whatever is waiting. Reports whether the
    /// UI has something new to draw.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(worker) = &self.worker {
            match worker.results.try_recv() {
                Ok((generation, (forge, result))) => {
                    self.busy = false;
                    if generation == self.generation.load(Ordering::Relaxed) {
                        self.loading = false;
                        changed = true;
                        if forge.is_some() {
                            self.forge = forge;
                        }
                        match result {
                            Ok(Done::Listed(origin, items)) => {
                                self.origin = Some(origin);
                                self.items = items;
                                self.message = None;
                            }
                            Ok(Done::Unlisted) => self.message = None,
                            Ok(Done::Fetched(branch)) => {
                                self.ready = Some(branch);
                                self.message = None;
                            }
                            Err(error) => {
                                tracing::warn!(
                                    category = "github_repo_items",
                                    detail = %error,
                                    "Listing pull requests and issues failed"
                                );
                                let gitlab =
                                    matches!(self.forge, Some(super::Forge::GitLab { .. }));
                                self.message = (!self.quiet || gitlab).then(|| error.to_string());
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.busy = false;
                    self.loading = false;
                    self.waiting = None;
                    self.worker = None;
                    self.message = Some(sheprd_github::Error::Worker("list").to_string());
                    return true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.busy
            && let Some(request) = self.waiting.take()
        {
            if self.worker.is_none() && !self.start() {
                return true;
            }
            if let Some(worker) = &self.worker {
                let (input, token, task) = request;
                self.busy = worker
                    .requests
                    .try_send((self.generation.load(Ordering::Relaxed), input, token, task))
                    .is_ok();
            }
        }
        changed
    }

    fn start(&mut self) -> bool {
        let (requests, incoming) = mpsc::sync_channel::<(u64, Target, Accounts, Task)>(1);
        let (outgoing, results) = mpsc::sync_channel(1);
        let current = self.generation.clone();
        let mut cooldown = self.cooldown;
        let started = thread::Builder::new()
            .name("herdr-repo-items".into())
            .spawn(move || {
                for (generation, input, token, task) in incoming {
                    let cancelled = || current.load(Ordering::Relaxed) != generation;
                    let result = match &task {
                        Task::List => {
                            let listing = fetch::list(&input, &token, cancelled, &mut cooldown);
                            let done = listing.listed.map(|listed| match listed {
                                Some((origin, items)) => Done::Listed(origin, items),
                                None => Done::Unlisted,
                            });
                            (listing.forge, done)
                        }
                        Task::Fetch(item) => (
                            None,
                            fetch::fetch_branch(&input, item, &cancelled)
                                .map(|()| Done::Fetched(item.branch())),
                        ),
                    };
                    if outgoing.send((generation, result)).is_err() {
                        break;
                    }
                }
            })
            .is_ok();
        if started {
            self.worker = Some(Worker { requests, results });
        } else {
            self.loading = false;
            self.message = Some(sheprd_github::Error::Worker("list").to_string());
        }
        started
    }
}
