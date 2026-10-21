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
    repo::{self, RepoInfo},
};
use crate::cache::{Cache, Kind};
use crate::github::Tokens;
use crate::teleport::{AgentKind, Host};
use chrono::{DateTime, Utc};
use herdr_client::ConnectTarget;
use secrecy::SecretString;
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

/// How often other applications' writes (agent-launcher's runs) are looked for.
const RELOAD: Duration = Duration::from_secs(5);
/// How often sources sync, as agent-launcher's default poll interval.
const SYNC: Duration = Duration::from_secs(60);

/// The worker's periods; tests shorten them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Timing {
    pub(crate) reload: Duration,
    pub(crate) sync: Duration,
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
pub(super) fn retry_wait(error: &Error, cooldown: Option<Duration>) -> Duration {
    let floor = match error {
        Error::Github(source) => match **source {
            crate::Error::GitHubRepositoryHidden { .. } => HIDDEN_BACKOFF,
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
pub(crate) struct Request {
    pub(crate) target: ConnectTarget,
    /// Any checkout of the repository on `target`.
    pub(crate) checkout: String,
    /// The Herdr workspace the view was opened from, whose repository new
    /// worktrees are created in.
    pub(crate) workspace_id: String,
    /// The accounts GitHub is read with; each source uses the one that
    /// reads its repository's owner.
    pub(crate) token: Option<Tokens>,
    /// The folder holding the databases, and agent-launcher's to carry
    /// over, in place of the platform's; tests set it.
    pub(crate) data_root: Option<PathBuf>,
    pub(crate) timing: Timing,
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
pub(crate) struct PullRequestLookup {
    pub(crate) number: u64,
    pub(crate) result: Arc<crate::Result<Option<crate::pull_request::PullRequest>>>,
}

/// How an action the user asked for ended.
#[derive(Clone, Debug)]
pub(crate) struct Notice {
    pub(crate) outcome: std::result::Result<&'static str, Arc<Error>>,
}

/// How one source is doing.
#[derive(Clone, Debug)]
pub(crate) enum SyncState {
    Syncing,
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
pub(crate) struct SourceStatus {
    pub(crate) key: SourceKey,
    pub(crate) state: SyncState,
}

/// Everything the view shows, replaced whole on every change.
#[derive(Clone, Debug, Default)]
pub(crate) struct Snapshot {
    pub(crate) repo: Option<RepoInfo>,
    pub(crate) access: Option<Access>,
    pub(crate) items: Arc<Vec<Item>>,
    pub(crate) runs: Arc<Vec<Run>>,
    pub(crate) sessions: Arc<Vec<HerdrSession>>,
    pub(crate) sources: Vec<SourceStatus>,
    /// Opening the repository or its database failed.
    pub(crate) error: Option<Arc<Error>>,
    /// agent-launcher's prompt profiles, read once when the view opened.
    pub(crate) profiles: Arc<Vec<Profile>>,
    /// The agents installed on the repository's host, once known.
    pub(crate) installed: Option<Arc<Vec<AgentKind>>>,
    pub(crate) installed_endpoint: Option<String>,
    pub(crate) installed_error: Option<Arc<Error>>,
    /// The newest pull request lookup.
    pub(crate) pull_request: Option<PullRequestLookup>,
    /// The repository's branches named for items, as each sync read them.
    pub(crate) branches: Arc<Vec<String>>,
    /// Each open pull request's review and checks, by repository and number,
    /// as the last sync of its source read them. Never stored.
    pub(crate) pr_status: Arc<HashMap<(String, u64), super::PrStatus>>,
}

pub(crate) struct Service {
    commands: SyncSender<Command>,
    mailbox: Arc<Mutex<Option<Snapshot>>>,
    notices: Arc<Mutex<VecDeque<Notice>>>,
    cancelled: Arc<AtomicBool>,
}

impl Service {
    pub(crate) fn load_agents(&self, endpoint: Option<String>, target: ConnectTarget) -> bool {
        self.commands
            .try_send(Command::LoadAgents { endpoint, target })
            .is_ok()
    }
    pub(crate) fn start(request: Request) -> std::io::Result<Self> {
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
            data_version: None,
            retry: Vec::new(),
            uncached: HashMap::new(),
            resume: HashMap::new(),
            gitlab_token: None,
            cooldown: None,
        };
        std::thread::Builder::new()
            .name("herdr-orchestrator".into())
            .spawn(move || worker.run(receiver))?;
        Ok(Self {
            commands,
            mailbox,
            notices,
            cancelled,
        })
    }

    /// Asks for `action`. A full queue means the worker is behind; the user
    /// is told, and nothing is queued silently.
    pub(crate) fn act(&self, action: Action) -> bool {
        self.commands.try_send(Command::Act(action)).is_ok()
    }

    /// Asks for pull request `number`'s status; a full queue drops it.
    pub(crate) fn load_pull_request(&self, number: u64) {
        let _ = self.commands.try_send(Command::LoadPullRequest(number));
    }

    /// Outcomes of finished actions since the last call.
    pub(crate) fn notices(&self) -> Vec<Notice> {
        self.notices
            .lock()
            .map(|mut notices| notices.drain(..).collect())
            .unwrap_or_default()
    }

    /// Asks for a sync now. A full queue already holds one, so it is dropped.
    pub(crate) fn refresh(&self, token: Option<Tokens>) {
        match self.commands.try_send(Command::Refresh(token)) {
            Ok(()) | Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
        }
    }

    /// The newest snapshot since the last call, if any.
    pub(crate) fn poll(&self) -> Option<Snapshot> {
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
    /// SQLite's `data_version` at the last load: it changes when another
    /// connection commits, so unchanged means nothing to reload.
    data_version: Option<i64>,
    /// Sources waiting out a failure, and until when.
    retry: Vec<(SourceKey, Instant)>,
    /// Listings shown but not written: capped, or the database is newer.
    uncached: HashMap<SourceKey, Vec<Item>>,
    /// Where each GitHub source's next incremental sync continues.
    resume: HashMap<SourceKey, github::Resume>,
    /// The GitLab token, once looked up; Refresh looks again, so a new
    /// `glab auth login` is picked up without reopening the view.
    gitlab_token: Option<Option<SecretString>>,
    cooldown: Option<Duration>,
}

impl Worker {
    fn run(mut self, commands: mpsc::Receiver<Command>) {
        if let Err(error) = self.open() {
            self.snapshot.error = Some(Arc::new(error));
            self.publish();
            return;
        }
        let token = self.request.token.clone();
        self.sync(token.as_ref());
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
                        .and_then(|host| installed(Some(&host), &self.cancelled));
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
        let result = crate::pull_request::by_number(
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
    /// slowly, over SSH most of all. A checkout that holds another
    /// repository now, or none, replaces or drops what was shown.
    fn open(&mut self) -> Result<()> {
        let host = Host::new(&self.request.target).map_err(|_| Error::UnsupportedHost)?;
        let cache = self.cache(false);
        let key = cache_key(&self.request.target, &self.request.checkout);
        let cached = cache.as_ref().and_then(|cache| {
            cache
                .get::<RepoInfo>(Kind::Repository, &key)
                .inspect_err(|error| tracing::debug!(%error, "Cannot read a cached repository"))
                .ok()
                .flatten()
                .map(|entry| entry.value)
        });
        let shown = cached
            .clone()
            .is_some_and(|info| match self.show(info, &host) {
                Ok(()) => true,
                Err(error) => {
                    tracing::debug!(%error, "Cannot show the cached repository");
                    false
                }
            });
        let info = match repo::detect(
            &self.request.target,
            &host,
            &self.request.checkout,
            &self.cancelled,
        ) {
            Ok(info) => info,
            Err(error) => {
                if !matches!(error, Error::Cancelled) {
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
        if !shown || cached.as_ref() != Some(&info) {
            self.show(info.clone(), &host)?;
            if let Some(cache) = cache.or_else(|| self.cache(true))
                && let Err(error) = cache.put(Kind::Repository, &key, &info)
            {
                tracing::debug!(%error, "Cannot cache the repository");
            }
        }
        match installed(self.host.as_ref(), &self.cancelled) {
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
        self.snapshot.sources = sources(&info)
            .into_iter()
            .map(|(key, supported)| SourceStatus {
                key,
                state: if supported {
                    SyncState::Syncing
                } else {
                    SyncState::Unsupported
                },
            })
            .collect();
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
        let Some(info) = self.snapshot.repo.clone() else {
            return;
        };
        if let Some(host) = &self.host
            && let Some(branches) = naming::local_branches(host, &info.main_root, &self.cancelled)
        {
            self.snapshot.branches = Arc::new(branches);
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
            self.set_state(&key, SyncState::Syncing);
            self.publish();
            // Only this source's own GitHub reply may set its pause: a delay
            // left by a pull request lookup or another source must not apply.
            self.cooldown = None;
            let state = match self.sync_source(&key, token) {
                Ok(state) => state,
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
                let (items, checkpoint) = beads::sync(host, key, &self.cancelled)?;
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
                    crate::github::graphql(
                        "orchestrator",
                        token,
                        query,
                        variables,
                        super::review::timeout(),
                        || cancelled.load(Ordering::Relaxed),
                        &mut self.cooldown,
                    )
                    .map_err(|error| match error {
                        crate::Error::PrCancelled => Error::Cancelled,
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
                let token = self
                    .gitlab_token
                    .get_or_insert_with(|| gitlab::token(&key.host));
                let scheme = self
                    .snapshot
                    .repo
                    .as_ref()
                    .and_then(|info| info.remote.as_ref())
                    .filter(|remote| remote.source == *key)
                    .map_or(gitlab::Scheme::Https, |remote| {
                        gitlab::Scheme::of_remote(&remote.url)
                    });
                let previous = store.checkpoint(key)?;
                let cancelled = &self.cancelled;
                let listing = gitlab::sync(
                    (key, scheme),
                    token.as_ref(),
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

    fn set_state(&mut self, key: &SourceKey, state: SyncState) {
        if let Some(status) = self.snapshot.sources.iter_mut().find(|s| s.key == *key) {
            status.state = state;
        }
    }
}

/// The app cache's key for the repository `checkout` on `target` holds.
pub(super) fn cache_key(target: &ConnectTarget, checkout: &str) -> String {
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
    Ok(AgentKind::ALL
        .into_iter()
        .filter(|kind| found.iter().any(|binary| binary == kind.binary()))
        .collect())
}

/// The sources a repository has, and whether this build can read each.
pub(super) fn sources(info: &RepoInfo) -> Vec<(SourceKey, bool)> {
    let remote = info.remote.as_ref().map(|remote| {
        let supported = match remote.source.provider {
            Provider::Github => remote.source.host == "github.com",
            Provider::Gitlab => true,
            Provider::Beads => false,
        };
        (remote.source.clone(), supported)
    });
    remote
        .into_iter()
        .chain(info.beads_source().map(|key| (key, true)))
        .collect()
}
