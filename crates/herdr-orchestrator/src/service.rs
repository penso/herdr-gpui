//! The orchestrator's worker: one thread per open view, owning the shared
//! database connection and every host script and network call. The UI sends
//! bounded commands and reads the latest [`Snapshot`] from a coalescing
//! mailbox on its tick; it never waits on this thread.

use super::{
    Access, Error, HerdrSession, Item, Provider, Result, Run, SourceKey, Store,
    actions::{Action, Site},
    beads, database_path, github, gitlab,
    location::{database_path_in, legacy_path, legacy_path_in},
    naming,
    prompt::{self, Profile},
    repo::{self, Extras, RepoInfo},
    ssh_slots,
};
use crate::cache::{Cache, Kind};
use chrono::{DateTime, Utc};
use herdr_client::ConnectTarget;
use herdr_github::Tokens;
use herdr_remote::teleport::{AgentKind, Host};
use secrecy::SecretString;
use sources::{recognize, statuses};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, RecvTimeoutError, SyncSender, TrySendError},
    },
    time::{Duration, Instant},
};

mod sources;

pub use sources::sources;

/// How often other applications' writes (agent-launcher's runs) are looked for.
const RELOAD: Duration = Duration::from_secs(5);
/// How often sources sync, as agent-launcher's default poll interval.
const SYNC: Duration = Duration::from_secs(60);

/// The worker's periods; tests shorten them.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub reload: Duration,
    pub sync: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            reload: RELOAD,
            sync: SYNC,
        }
    }
}
/// After a failure, a source waits this long before trying again.
const BACKOFF: Duration = Duration::from_secs(300);
/// A repository GitHub would not show this sign-in waits for the user to
/// install the App or change token; Refresh asks sooner.
const HIDDEN_BACKOFF: Duration = Duration::from_secs(1800);
const COMMANDS: usize = 8;

/// How long a failed source waits. A rate limit or refused token pauses as
/// long as the provider asked; a repository the sign-in cannot see waits for
/// a user action. The `cooldown` applies only to a provider's own failures,
/// so a Beads failure never inherits a pause a GitHub request left behind.
pub fn retry_wait(error: &Error, cooldown: Option<Duration>) -> Duration {
    let floor = match error {
        Error::Github(source) => match **source {
            herdr_github::Error::RepositoryHidden { .. } => HIDDEN_BACKOFF,
            _ => BACKOFF,
        },
        Error::Gitlab(source) if source.needs_user() => HIDDEN_BACKOFF,
        Error::Gitlab(_) => BACKOFF,
        _ => return BACKOFF,
    };
    cooldown.map_or(floor, |wait| wait.max(floor))
}
/// Actions running at once; more wait for the user to try again.
const MAX_ACTIONS: usize = 4;
/// Outcomes kept for the view between its polls.
const MAX_NOTICES: usize = 8;

/// Where the view's repository is and whom to read GitHub as.
#[derive(Clone)]
pub struct Request {
    pub target: ConnectTarget,
    /// Any checkout of the repository on `target`.
    pub checkout: String,
    /// The Herdr workspace the view was opened from, whose repository new
    /// worktrees are created in.
    pub workspace_id: String,
    /// The accounts GitHub is read with; each source uses the one that
    /// reads its repository's owner.
    pub token: Option<Tokens>,
    /// The folder holding the databases, and agent-launcher's to carry
    /// over, in place of the platform's; tests set it.
    pub data_root: Option<PathBuf>,
    pub timing: Timing,
}

enum Command {
    /// Sync every source now, reading GitHub as `token`.
    Refresh(Option<Tokens>),
    /// Do what the user asked, on a thread of its own.
    Act(Action),
    /// An action finished: read the database again, and sync when it changed
    /// a source.
    Acted { resync: bool },
    /// Look up a pull request's status, checks, and merge state by number.
    LoadPullRequest(u64),
    LoadAgents {
        endpoint: Option<String>,
        target: ConnectTarget,
    },
}

/// A pull request looked up for the open item.
#[derive(Clone, Debug)]
pub struct PullRequestLookup {
    pub number: u64,
    pub result: Arc<herdr_github::Result<Option<herdr_github::pull_request::PullRequest>>>,
}

/// How an action the user asked for ended.
#[derive(Clone, Debug)]
pub struct Notice {
    pub outcome: std::result::Result<&'static str, Arc<Error>>,
}

/// How one source is doing.
#[derive(Clone, Debug)]
pub enum SyncState {
    /// With the count its last sync reported, shown until this one ends.
    Syncing {
        count: Option<usize>,
    },
    Synced {
        at: DateTime<Utc>,
        count: usize,
    },
    /// Larger than one sync reads; shown, not cached.
    Capped {
        count: usize,
    },
    /// GitHub needs a signed-in account.
    SignIn,
    /// A provider or host this build cannot read yet, such as GitHub Enterprise.
    Unsupported,
    Failed(Arc<Error>),
}

#[derive(Clone, Debug)]
pub struct SourceStatus {
    pub key: SourceKey,
    pub state: SyncState,
}

/// Everything the view shows, replaced whole on every change.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub repo: Option<RepoInfo>,
    pub access: Option<Access>,
    pub items: Arc<Vec<Item>>,
    pub runs: Arc<Vec<Run>>,
    pub sessions: Arc<Vec<HerdrSession>>,
    pub sources: Vec<SourceStatus>,
    /// Opening the repository or its database failed.
    pub error: Option<Arc<Error>>,
    /// agent-launcher's prompt profiles, read once when the view opened.
    pub profiles: Arc<Vec<Profile>>,
    /// The agents installed on the repository's host, once known.
    pub installed: Option<Arc<Vec<AgentKind>>>,
    pub installed_endpoint: Option<String>,
    pub installed_error: Option<Arc<Error>>,
    /// The newest pull request lookup.
    pub pull_request: Option<PullRequestLookup>,
    /// The repository's branches named for items, as each sync read them.
    pub branches: Arc<Vec<String>>,
    /// Each open pull request's review and checks, by repository and number,
    /// as the last sync of its source read them. Never stored.
    pub pr_status: Arc<HashMap<(String, u64), super::PrStatus>>,
}

pub struct Service {
    commands: SyncSender<Command>,
    mailbox: Arc<Mutex<Option<Snapshot>>>,
    notices: Arc<Mutex<VecDeque<Notice>>>,
    cancelled: Arc<AtomicBool>,
}

impl Service {
    pub fn load_agents(&self, endpoint: Option<String>, target: ConnectTarget) -> bool {
        self.commands
            .try_send(Command::LoadAgents { endpoint, target })
            .is_ok()
    }
    pub fn start(request: Request) -> std::io::Result<Self> {
        let (service, worker, receiver) = Self::prepare(request);
        std::thread::Builder::new()
            .name("herdr-orchestrator".into())
            .spawn(move || worker.run(receiver))?;
        Ok(service)
    }

    /// The service and the worker it will talk to, not yet started.
    fn prepare(request: Request) -> (Self, Worker, mpsc::Receiver<Command>) {
        let (commands, receiver) = mpsc::sync_channel(COMMANDS);
        let mailbox = Arc::new(Mutex::new(None));
        let notices = Arc::new(Mutex::new(VecDeque::new()));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker = Worker {
            request,
            mailbox: mailbox.clone(),
            notices: notices.clone(),
            commands: commands.clone(),
            actions: Arc::new(AtomicUsize::new(0)),
            site: None,
            cancelled: cancelled.clone(),
            snapshot: Snapshot::default(),
            store: None,
            host: None,
            unreached: false,
            branches_fresh: false,
            data_version: None,
            retry: Vec::new(),
            uncached: HashMap::new(),
            resume: HashMap::new(),
            gitlab_token: None,
            cooldown: None,
        };
        let service = Self {
            commands,
            mailbox,
            notices,
            cancelled,
        };
        (service, worker, receiver)
    }

    /// Asks for `action`. A full queue means the worker is behind; the user
    /// is told, and nothing is queued silently.
    pub fn act(&self, action: Action) -> bool {
        self.commands.try_send(Command::Act(action)).is_ok()
    }

    /// Asks for pull request `number`'s status; a full queue drops it.
    pub fn load_pull_request(&self, number: u64) {
        let _ = self.commands.try_send(Command::LoadPullRequest(number));
    }

    /// Outcomes of finished actions since the last call.
    pub fn notices(&self) -> Vec<Notice> {
        self.notices
            .lock()
            .map(|mut notices| notices.drain(..).collect())
            .unwrap_or_default()
    }

    /// Asks for a sync now. A full queue already holds one, so it is dropped.
    pub fn refresh(&self, token: Option<Tokens>) {
        match self.commands.try_send(Command::Refresh(token)) {
            Ok(()) | Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
        }
    }

    /// The newest snapshot since the last call, if any.
    pub fn poll(&self) -> Option<Snapshot> {
        self.mailbox.lock().ok()?.take()
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        // The worker sees this at its next step or script check; it is never
        // joined here, on the UI thread.
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

struct Worker {
    request: Request,
    mailbox: Arc<Mutex<Option<Snapshot>>>,
    notices: Arc<Mutex<VecDeque<Notice>>>,
    /// The worker's own queue, for action threads to report back on.
    commands: SyncSender<Command>,
    actions: Arc<AtomicUsize>,
    site: Option<Site>,
    cancelled: Arc<AtomicBool>,
    snapshot: Snapshot,
    store: Option<Store>,
    host: Option<Host>,
    /// The host could not be reached to find the repository: what was shown
    /// stays, beside the failure, and each sync looks again first.
    unreached: bool,
    /// The branches were read with the repository, so the sync that follows
    /// need not read them again.
    branches_fresh: bool,
    /// SQLite's `data_version` at the last load: it changes when another
    /// connection commits, so unchanged means nothing to reload.
    data_version: Option<i64>,
    /// Sources waiting out a failure, and until when.
    retry: Vec<(SourceKey, Instant)>,
    /// Listings shown but not written: capped, or the database is newer.
    uncached: HashMap<SourceKey, Vec<Item>>,
    /// Where each GitHub source's next incremental sync continues.
    resume: HashMap<SourceKey, github::Resume>,
    /// The GitLab token, once looked up, and the instance it is for; Refresh
    /// looks again, so a new `glab auth login` is picked up without
    /// reopening the view.
    gitlab_token: Option<(String, Option<SecretString>)>,
    cooldown: Option<Duration>,
}

impl Worker {
    fn run(mut self, commands: mpsc::Receiver<Command>) {
        if let Err(error) = self.open() {
            self.snapshot.error = Some(Arc::new(error));
            self.publish();
            return;
        }
        // The host was just found unreachable: the next sync looks again.
        if !self.unreached {
            let token = self.request.token.clone();
            self.sync(token.as_ref());
        }
        let Timing { reload, sync } = self.request.timing;
        let mut next_sync = Instant::now() + sync;
        while !self.cancelled() {
            let wait = reload.min(next_sync.saturating_duration_since(Instant::now()));
            match commands.recv_timeout(wait) {
                Ok(Command::Refresh(token)) => {
                    self.request.token = token;
                    self.gitlab_token = None;
                    self.retry.clear();
                    let token = self.request.token.clone();
                    self.sync(token.as_ref());
                    next_sync = Instant::now() + sync;
                }
                Ok(Command::Act(action)) => self.act(action),
                Ok(Command::LoadPullRequest(number)) => self.load_pull_request(number),
                Ok(Command::LoadAgents { endpoint, target }) => {
                    self.snapshot.installed_endpoint = endpoint;
                    self.snapshot.installed = None;
                    self.snapshot.installed_error = None;
                    self.publish();
                    let result = Host::new(&target)
                        .map_err(|_| Error::UnsupportedHost)
                        .and_then(|host| {
                            ssh_slots::run(&target, &self.cancelled, || {
                                installed(Some(&host), &self.cancelled)
                            })
                        });
                    match result {
                        Ok(agents) => self.snapshot.installed = Some(Arc::new(agents)),
                        Err(error) => self.snapshot.installed_error = Some(Arc::new(error)),
                    }
                    self.publish();
                }
                Ok(Command::Acted { resync }) => {
                    if resync {
                        let token = self.request.token.clone();
                        self.sync(token.as_ref());
                    } else {
                        self.reload_if_changed();
                    }
                }
                Err(RecvTimeoutError::Timeout) if Instant::now() >= next_sync => {
                    let token = self.request.token.clone();
                    self.sync(token.as_ref());
                    next_sync = Instant::now() + sync;
                }
                Err(RecvTimeoutError::Timeout) => self.reload_if_changed(),
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// Starts `action` on a thread of its own, at most [`MAX_ACTIONS`] at once.
    fn act(&mut self, action: Action) {
        let Some(site) = self.site.clone() else {
            self.notice(Err(Arc::new(Error::NotARepository)));
            return;
        };
        if self.snapshot.access != Some(Access::ReadWrite) {
            let found = match self.snapshot.access {
                Some(Access::ReadOnly { found }) => found,
                _ => 0,
            };
            self.notice(Err(Arc::new(Error::NewerSchema {
                found,
                known: super::SCHEMA_VERSION,
            })));
            return;
        }
        if self.actions.fetch_add(1, Ordering::SeqCst) >= MAX_ACTIONS {
            self.actions.fetch_sub(1, Ordering::SeqCst);
            self.notice(Err(Arc::new(Error::Busy)));
            return;
        }
        let resync = matches!(action, Action::DeleteBead { .. });
        let (notices, commands, actions) = (
            self.notices.clone(),
            self.commands.clone(),
            self.actions.clone(),
        );
        let spawned = spawn_action(move |cancelled| {
            let outcome = super::actions::perform(&site, &action, cancelled).map_err(Arc::new);
            push(&notices, Notice { outcome });
            actions.fetch_sub(1, Ordering::SeqCst);
            let _ = commands.try_send(Command::Acted { resync });
        });
        if let Err(error) = spawned {
            self.actions.fetch_sub(1, Ordering::SeqCst);
            self.notice(Err(Arc::new(Error::Worker(error))));
        }
    }

    fn load_pull_request(&mut self, number: u64) {
        let remote = self
            .snapshot
            .repo
            .as_ref()
            .and_then(|info| info.remote.as_ref())
            .filter(|remote| remote.source.provider == Provider::Github);
        let (Some(remote), Some(token)) = (remote, self.request.token.clone()) else {
            return;
        };
        let Some((owner, repo)) = remote.source.repository.split_once('/') else {
            return;
        };
        let cancelled = self.cancelled.clone();
        let result = herdr_github::pull_request::by_number(
            owner,
            repo,
            number,
            token.for_owner(owner),
            || cancelled.load(Ordering::Relaxed),
            &mut self.cooldown,
        );
        self.snapshot.pull_request = Some(PullRequestLookup {
            number,
            result: Arc::new(result),
        });
        self.publish();
    }

    fn notice(&self, outcome: std::result::Result<&'static str, Arc<Error>>) {
        push(&self.notices, Notice { outcome });
    }

    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    fn publish(&self) {
        if let Ok(mut mailbox) = self.mailbox.lock() {
            *mailbox = Some(self.snapshot.clone());
        }
    }

    /// Shows what the last run found at once, the repository it detected
    /// and its database, then finds the repository again: git may answer
    /// slowly, over SSH most of all.
    fn open(&mut self) -> Result<()> {
        let host = Host::new(&self.request.target).map_err(|_| Error::UnsupportedHost)?;
        let key = cache_key(&self.request.target, &self.request.checkout);
        let cached = self.cache(false).and_then(|cache| {
            cache
                .get::<RepoInfo>(Kind::Repository, &key)
                .inspect_err(|error| tracing::debug!(%error, "Cannot read a cached repository"))
                .ok()
                .flatten()
                .map(|entry| entry.value)
        });
        if let Some(info) = cached
            && let Err(error) = self.show(info, &host)
        {
            // `show` may have set the repository before failing: drop it, so
            // `find` shows it again and reports what failed.
            tracing::debug!(%error, "Cannot show the cached repository");
            self.forget_repository();
        }
        self.find(&host)
    }

    /// Finds the repository in the checkout, and shows and caches it unless
    /// it is what is shown. A checkout that holds another repository now, or
    /// none, replaces or drops what was shown. A host that could not be
    /// reached answered nothing about the checkout, so what was shown stays,
    /// with the failure beside it, until a later sync finds it.
    fn find(&mut self, host: &Host) -> Result<()> {
        let target = &self.request.target;
        let binaries = AgentKind::ALL.map(AgentKind::binary);
        let detected = ssh_slots::run(target, &self.cancelled, || {
            repo::detect_opening(
                target,
                host,
                &self.request.checkout,
                &binaries,
                &self.cancelled,
            )
        });
        self.found(host, detected)
    }

    /// Acts on what [`Worker::find`] detected.
    fn found(&mut self, host: &Host, detected: Result<(RepoInfo, Extras)>) -> Result<()> {
        let cache = self.cache(false);
        let key = cache_key(&self.request.target, &self.request.checkout);
        let (mut info, extras) = match detected {
            Ok(found) => found,
            Err(Error::Script { operation, source }) if source.host_unreached() => {
                self.unreached = true;
                self.snapshot.error = Some(Arc::new(Error::Script { operation, source }));
                self.publish();
                return Ok(());
            }
            Err(error) => {
                if !matches!(error, Error::Cancelled) {
                    self.unreached = false;
                    self.forget_repository();
                    if let Some(cache) = &cache
                        && let Err(error) = cache.remove(Kind::Repository, &key)
                    {
                        tracing::debug!(%error, "Cannot drop a cached repository");
                    }
                }
                return Err(error);
            }
        };
        recognize(&mut info, gitlab::cli_hosts::read);
        if std::mem::take(&mut self.unreached) {
            self.snapshot.error = None;
        }
        if self.snapshot.repo.as_ref() != Some(&info) {
            self.show(info.clone(), host)?;
            if let Some(cache) = cache.or_else(|| self.cache(true))
                && let Err(error) = cache.put(Kind::Repository, &key, &info)
            {
                tracing::debug!(%error, "Cannot cache the repository");
            }
        }
        if let Some(branches) = extras.branches {
            self.snapshot.branches = Arc::new(branches);
            self.branches_fresh = true;
        }
        let found = match extras.programs {
            Some(binaries) => Ok(agents(&binaries)),
            None => ssh_slots::run(&self.request.target, &self.cancelled, || {
                installed(self.host.as_ref(), &self.cancelled)
            }),
        };
        match found {
            Ok(agents) => self.snapshot.installed = Some(Arc::new(agents)),
            Err(error) => self.snapshot.installed_error = Some(Arc::new(error)),
        }
        self.publish();
        Ok(())
    }

    /// The app's cache database, beside the databases in `data_root` when
    /// a test sets one; only made when `create` is set, so a folder outside
    /// Git leaves nothing behind. Without it nothing is cached and nothing
    /// fails.
    fn cache(&self, create: bool) -> Option<Cache> {
        let path = match &self.request.data_root {
            Some(root) => crate::cache::path_in(root),
            None => crate::cache::path()
                .inspect_err(|error| tracing::debug!(%error, "No cache"))
                .ok()?,
        };
        if !create && !path.is_file() {
            return None;
        }
        Cache::open_at(&path)
            .inspect_err(|error| tracing::debug!(%error, "Cannot open the cache"))
            .ok()
    }

    /// Drops a repository shown from the cache that git no longer finds.
    fn forget_repository(&mut self) {
        self.store = None;
        self.site = None;
        self.data_version = None;
        self.snapshot = Snapshot::default();
    }

    /// Opens `info`'s database, carrying agent-launcher's over the first
    /// time, and publishes what it holds.
    fn show(&mut self, info: RepoInfo, host: &Host) -> Result<()> {
        let (path, legacy) = match &self.request.data_root {
            Some(root) => (
                database_path_in(root, &info.repository),
                Some(legacy_path_in(root, &info.repository)),
            ),
            None => (
                database_path(&info.repository).ok_or(Error::NoDataDirectory)?,
                legacy_path(&info.repository),
            ),
        };
        if let Some(legacy) = legacy.filter(|legacy| !path.exists() && legacy.is_file())
            && let Err(error) = Store::import(&legacy, &path)
        {
            tracing::warn!(
                category = "orchestrator",
                %error,
                "Cannot carry over agent-launcher's database"
            );
        }
        let store = Store::open(&path)?;
        self.snapshot.access = Some(store.access());
        self.snapshot.sources = statuses(&info);
        self.site = Some(Site {
            host: host.clone(),
            target: self.request.target.clone(),
            main_root: info.main_root.clone(),
            database: path,
        });
        if let Some(home) = std::env::var_os("HOME") {
            self.snapshot.profiles = Arc::new(prompt::load_profiles(std::path::Path::new(&home)));
        }
        self.snapshot.repo = Some(info);
        // The checks last read, until the first sync reads them again.
        self.snapshot.pr_status = Arc::new(store.pr_statuses().unwrap_or_else(|error| {
            tracing::debug!(%error, "Cannot read stored pull request checks");
            HashMap::new()
        }));
        self.store = Some(store);
        self.host = Some(host.clone());
        self.reload()?;
        self.publish();
        Ok(())
    }

    /// Reads items and runs again, then publishes.
    fn reload(&mut self) -> Result<()> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        self.data_version = Some(store.data_version()?);
        let mut items = store.items()?;
        if !self.uncached.is_empty() {
            items.retain(|item| !self.uncached.contains_key(&item.key.source));
            items.extend(self.uncached.values().flatten().cloned());
        }
        self.snapshot.items = Arc::new(items);
        self.snapshot.runs = Arc::new(store.runs()?);
        self.snapshot.sessions = Arc::new(store.sessions()?);
        Ok(())
    }

    fn reload_if_changed(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        let changed = match store.data_version() {
            Ok(version) => Some(version) != self.data_version,
            Err(_) => true,
        };
        if changed {
            if let Err(error) = self.reload() {
                self.snapshot.error = Some(Arc::new(error));
            }
            self.publish();
        }
    }

    fn sync(&mut self, token: Option<&Tokens>) {
        if self.unreached {
            let found = Host::new(&self.request.target)
                .map_err(|_| Error::UnsupportedHost)
                .and_then(|host| self.find(&host));
            if let Err(error) = found
                && !matches!(error, Error::Cancelled)
            {
                self.snapshot.error = Some(Arc::new(error));
                self.publish();
            }
            // Still unreached: the sources' scripts would not reach it either.
            if self.unreached {
                return;
            }
        }
        self.recognize(gitlab::cli_hosts::read);
        let Some(info) = self.snapshot.repo.clone() else {
            return;
        };
        let (target, cancelled) = (&self.request.target, &self.cancelled);
        if !std::mem::take(&mut self.branches_fresh)
            && let Some(host) = &self.host
        {
            // The branches last read stay when they cannot be read now.
            match ssh_slots::run(target, cancelled, || {
                naming::local_branches(host, &info.main_root, cancelled)
            }) {
                Ok(branches) => self.snapshot.branches = Arc::new(branches),
                Err(error) => tracing::debug!(%error, "Cannot read the branches"),
            }
        }
        for (key, supported) in sources(&info) {
            if self.cancelled() {
                return;
            }
            let now = Instant::now();
            self.retry.retain(|(_, until)| *until > now);
            if !supported || self.retry.iter().any(|(waiting, _)| *waiting == key) {
                continue;
            }
            self.set_syncing(&key);
            self.publish();
            // Only this source's own GitHub reply may set its pause: a delay
            // left by a pull request lookup or another source must not apply.
            self.cooldown = None;
            let state = match self.sync_source(&key, token) {
                // The view shows no sync times, so the log keeps them.
                Ok(state) => {
                    tracing::debug!(
                        category = "orchestrator_sync",
                        provider = ?key.provider,
                        host = key.host.as_str(),
                        repository = key.repository.as_str(),
                        elapsed_ms = now.elapsed().as_millis(),
                        state = ?state,
                        "Orchestrator source synced"
                    );
                    state
                }
                Err(Error::Cancelled) => return,
                Err(error) => {
                    let wait = retry_wait(&error, self.cooldown.take());
                    tracing::warn!(
                        category = "orchestrator_sync",
                        provider = ?key.provider,
                        host = key.host.as_str(),
                        repository = key.repository.as_str(),
                        retry_in_secs = wait.as_secs(),
                        error = %error,
                        "Orchestrator source sync failed"
                    );
                    self.retry.push((key.clone(), Instant::now() + wait));
                    SyncState::Failed(Arc::new(error))
                }
            };
            self.set_state(&key, state);
            if let Err(error) = self.reload() {
                self.snapshot.error = Some(Arc::new(error));
            }
            self.publish();
        }
    }

    fn sync_source(&mut self, key: &SourceKey, token: Option<&Tokens>) -> Result<SyncState> {
        let (Some(store), Some(host)) = (self.store.as_mut(), self.host.as_ref()) else {
            return Ok(SyncState::Unsupported);
        };
        let writable = store.access() == Access::ReadWrite;
        match key.provider {
            Provider::Beads => {
                let (items, checkpoint) =
                    ssh_slots::run(&self.request.target, &self.cancelled, || {
                        beads::sync(host, key, &self.cancelled)
                    })?;
                let count = items.len();
                if writable {
                    store.replace_items(key, &items, &checkpoint)?;
                    self.uncached.remove(key);
                } else {
                    self.uncached.insert(key.clone(), items);
                }
                Ok(SyncState::Synced {
                    at: Utc::now(),
                    count,
                })
            }
            Provider::Github => {
                let Some(tokens) = token else {
                    return Ok(SyncState::SignIn);
                };
                let owner = key
                    .repository
                    .split_once('/')
                    .map_or(key.repository.as_str(), |(owner, _)| owner);
                let token = tokens.for_owner(owner);
                let previous = store.checkpoint(key)?;
                // What the last complete sync stored, for reading only the
                // changes since; a read-only store keeps none.
                let cached = match (&previous, writable) {
                    (Some(_), true) => Some(
                        store
                            .items()?
                            .into_iter()
                            .filter(|item| item.key.source == *key)
                            .collect(),
                    ),
                    _ => None,
                };
                // Only while the stored checkpoint is still the one this
                // worker wrote: agent-launcher shares the database and may
                // have stored an older listing since.
                let resume = self.resume.get(key).copied().filter(|resume| {
                    previous
                        .as_ref()
                        .and_then(|checkpoint| checkpoint.updated_at)
                        == Some(resume.oldest())
                });
                let cancelled = &self.cancelled;
                let listing = github::sync(
                    key,
                    token,
                    previous.as_ref(),
                    (cached, resume),
                    &|| cancelled.load(Ordering::Relaxed),
                    &mut self.cooldown,
                )?;
                let count = listing.items.len();
                // Reviews and checks change with every CI run, so they are
                // read again each sync. Failing to read them keeps the last
                // ones rather than failing the listing.
                let statuses = super::review::fetch(&key.repository, |query, variables| {
                    herdr_github::graphql(
                        "orchestrator",
                        token,
                        query,
                        variables,
                        super::review::timeout(),
                        || cancelled.load(Ordering::Relaxed),
                        &mut self.cooldown,
                    )
                    .map_err(|error| match error {
                        herdr_github::Error::PullRequest(
                            herdr_github::pull_request::Error::Cancelled,
                        ) => Error::Cancelled,
                        error => Error::Github(Box::new(error)),
                    })
                });
                match statuses {
                    Ok(statuses) => {
                        if writable
                            && let Err(error) =
                                store.replace_pr_statuses(&key.repository, &statuses)
                        {
                            tracing::warn!(%error, "Cannot keep pull request checks");
                        }
                        let mut all = (*self.snapshot.pr_status).clone();
                        all.retain(|(repository, _), _| *repository != key.repository);
                        all.extend(
                            statuses
                                .into_iter()
                                .map(|(number, status)| ((key.repository.clone(), number), status)),
                        );
                        self.snapshot.pr_status = Arc::new(all);
                    }
                    Err(Error::Cancelled) => return Err(Error::Cancelled),
                    Err(error) => tracing::warn!(%error, "Cannot read pull request checks"),
                }
                // A listing that is not stored is no place to resume from.
                self.resume.remove(key);
                if !listing.complete || !writable {
                    self.uncached.insert(key.clone(), listing.items);
                    return Ok(if listing.complete {
                        SyncState::Synced {
                            at: Utc::now(),
                            count,
                        }
                    } else {
                        SyncState::Capped { count }
                    });
                }
                store.replace_items(key, &listing.items, &listing.checkpoint)?;
                self.resume.insert(key.clone(), listing.resume);
                self.uncached.remove(key);
                Ok(SyncState::Synced {
                    at: Utc::now(),
                    count,
                })
            }
            Provider::Gitlab => {
                let scheme = self
                    .snapshot
                    .repo
                    .as_ref()
                    .and_then(|info| info.remote.as_ref())
                    .filter(|remote| remote.source == *key)
                    .map_or(gitlab::Scheme::Https, |remote| {
                        gitlab::Scheme::of_remote(&remote.url)
                    });
                let api = gitlab::Api::resolve(&key.host, scheme);
                if self
                    .gitlab_token
                    .as_ref()
                    .is_none_or(|(instance, _)| *instance != api.instance)
                {
                    let token = gitlab::token(&api.instance);
                    self.gitlab_token = Some((api.instance.clone(), token));
                }
                let token = self
                    .gitlab_token
                    .as_ref()
                    .and_then(|(_, token)| token.as_ref());
                let previous = store.checkpoint(key)?;
                let cancelled = &self.cancelled;
                let listing = gitlab::sync(
                    (key, &api),
                    token,
                    previous.as_ref(),
                    &|| cancelled.load(Ordering::Relaxed),
                    &mut self.cooldown,
                )?;
                let count = listing.items.len();
                if !listing.complete || !writable {
                    self.uncached.insert(key.clone(), listing.items);
                    return Ok(if listing.complete {
                        SyncState::Synced {
                            at: Utc::now(),
                            count,
                        }
                    } else {
                        SyncState::Capped { count }
                    });
                }
                store.replace_items(key, &listing.items, &listing.checkpoint)?;
                self.uncached.remove(key);
                Ok(SyncState::Synced {
                    at: Utc::now(),
                    count,
                })
            }
        }
    }

    /// Reads the shown remote as GitLab once glab knows its host, such as
    /// through an `ssh_host` added after the repository was found.
    fn recognize(&mut self, cli: impl FnOnce() -> Vec<gitlab::cli_hosts::CliHost>) {
        let Some(mut info) = self.snapshot.repo.clone() else {
            return;
        };
        if recognize(&mut info, cli) {
            self.snapshot.sources = statuses(&info);
            self.snapshot.repo = Some(info);
        }
    }

    fn set_state(&mut self, key: &SourceKey, state: SyncState) {
        if let Some(status) = self.snapshot.sources.iter_mut().find(|s| s.key == *key) {
            status.state = state;
        }
    }
}

/// The app cache's key for the repository `checkout` on `target` holds.
pub fn cache_key(target: &ConnectTarget, checkout: &str) -> String {
    format!("{target:?}\n{checkout}")
}

/// Once accepted, an action completes even when its view stops observing it.
fn spawn_action(
    action: impl FnOnce(&AtomicBool) + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("herdr-orchestrator-action".into())
        .spawn(move || action(&AtomicBool::new(false)))
}

#[cfg(test)]
mod tests;

/// Adds `notice`, dropping the oldest past [`MAX_NOTICES`].
fn push(notices: &Mutex<VecDeque<Notice>>, notice: Notice) {
    if let Ok(mut notices) = notices.lock() {
        if notices.len() >= MAX_NOTICES {
            notices.pop_front();
        }
        notices.push_back(notice);
    }
}

/// The agents installed on `host`, in [`AgentKind::ALL`] order.
fn installed(host: Option<&Host>, cancelled: &AtomicBool) -> Result<Vec<AgentKind>> {
    let binaries = AgentKind::ALL.map(AgentKind::binary);
    let found = host
        .ok_or(Error::UnsupportedHost)?
        .installed_programs(&binaries, cancelled)
        .map_err(super::error::script("Discovering installed agents"))?;
    Ok(agents(&found))
}

/// The agents whose binaries are among `found`.
fn agents(found: &[String]) -> Vec<AgentKind> {
    AgentKind::ALL
        .into_iter()
        .filter(|kind| found.iter().any(|binary| binary == kind.binary()))
        .collect()
}
