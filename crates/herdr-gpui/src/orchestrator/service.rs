//! The orchestrator's worker: one thread per open view, owning the shared
//! database connection and every host script and network call. The UI sends
//! bounded commands and reads the latest [`Snapshot`] from a coalescing
//! mailbox on its tick; it never waits on this thread.

use super::{
    Access, Error, HerdrSession, Item, Provider, Result, Run, SourceKey, Store, beads,
    database_path, github,
    location::database_path_in,
    repo::{self, RepoInfo},
};
use crate::teleport::Host;
use chrono::{DateTime, Utc};
use herdr_client::ConnectTarget;
use secrecy::SecretString;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
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
const COMMANDS: usize = 8;

/// Where the view's repository is and whom to read GitHub as.
#[derive(Clone)]
pub(crate) struct Request {
    pub(crate) target: ConnectTarget,
    /// Any checkout of the repository on `target`.
    pub(crate) checkout: String,
    pub(crate) token: Option<Arc<SecretString>>,
    /// agent-launcher's data directory parent; `None` for the platform's.
    pub(crate) data_root: Option<PathBuf>,
    pub(crate) timing: Timing,
}

enum Command {
    /// Sync every source now, reading GitHub as `token`.
    Refresh(Option<Arc<SecretString>>),
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
    /// A provider or host this build cannot read yet, such as GitLab.
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
}

pub(crate) struct Service {
    commands: SyncSender<Command>,
    mailbox: Arc<Mutex<Option<Snapshot>>>,
    cancelled: Arc<AtomicBool>,
}

impl Service {
    pub(crate) fn start(request: Request) -> std::io::Result<Self> {
        let (commands, receiver) = mpsc::sync_channel(COMMANDS);
        let mailbox = Arc::new(Mutex::new(None));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker = Worker {
            request,
            mailbox: mailbox.clone(),
            cancelled: cancelled.clone(),
            snapshot: Snapshot::default(),
            store: None,
            host: None,
            data_version: None,
            retry: Vec::new(),
            uncached: HashMap::new(),
            cooldown: None,
        };
        std::thread::Builder::new()
            .name("herdr-orchestrator".into())
            .spawn(move || worker.run(receiver))?;
        Ok(Self {
            commands,
            mailbox,
            cancelled,
        })
    }

    /// Asks for a sync now. A full queue already holds one, so it is dropped.
    pub(crate) fn refresh(&self, token: Option<Arc<SecretString>>) {
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
        self.sync(token.as_deref());
        let Timing { reload, sync } = self.request.timing;
        let mut next_sync = Instant::now() + sync;
        while !self.cancelled() {
            let wait = reload.min(next_sync.saturating_duration_since(Instant::now()));
            match commands.recv_timeout(wait) {
                Ok(Command::Refresh(token)) => {
                    self.request.token = token;
                    self.retry.clear();
                    let token = self.request.token.clone();
                    self.sync(token.as_deref());
                    next_sync = Instant::now() + sync;
                }
                Err(RecvTimeoutError::Timeout) if Instant::now() >= next_sync => {
                    let token = self.request.token.clone();
                    self.sync(token.as_deref());
                    next_sync = Instant::now() + sync;
                }
                Err(RecvTimeoutError::Timeout) => self.reload_if_changed(),
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    fn publish(&self) {
        if let Ok(mut mailbox) = self.mailbox.lock() {
            *mailbox = Some(self.snapshot.clone());
        }
    }

    /// Finds the repository, opens its database, and shows the cache.
    fn open(&mut self) -> Result<()> {
        let host = Host::new(&self.request.target).map_err(|_| Error::UnsupportedHost)?;
        let info = repo::detect(
            &self.request.target,
            &host,
            &self.request.checkout,
            &self.cancelled,
        )?;
        let path = match &self.request.data_root {
            Some(root) => database_path_in(root, &info.repository),
            None => database_path(&info.repository).ok_or(Error::NoDataDirectory)?,
        };
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
        self.snapshot.repo = Some(info);
        self.store = Some(store);
        self.host = Some(host);
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

    fn sync(&mut self, token: Option<&SecretString>) {
        let Some(info) = self.snapshot.repo.clone() else {
            return;
        };
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
            let state = match self.sync_source(&key, token) {
                Ok(state) => state,
                Err(Error::Cancelled) => return,
                Err(error) => {
                    self.retry.push((key.clone(), Instant::now() + BACKOFF));
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

    fn sync_source(&mut self, key: &SourceKey, token: Option<&SecretString>) -> Result<SyncState> {
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
                let Some(token) = token else {
                    return Ok(SyncState::SignIn);
                };
                let previous = store.checkpoint(key)?;
                let cancelled = &self.cancelled;
                let listing = github::sync(
                    key,
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
            Provider::Gitlab => Ok(SyncState::Unsupported),
        }
    }

    fn set_state(&mut self, key: &SourceKey, state: SyncState) {
        if let Some(status) = self.snapshot.sources.iter_mut().find(|s| s.key == *key) {
            status.state = state;
        }
    }
}

/// The sources a repository has, and whether this build can read each.
pub(super) fn sources(info: &RepoInfo) -> Vec<(SourceKey, bool)> {
    let remote = info.remote.as_ref().map(|remote| {
        let supported =
            remote.source.provider == Provider::Github && remote.source.host == "github.com";
        (remote.source.clone(), supported)
    });
    remote
        .into_iter()
        .chain(info.beads_source().map(|key| (key, true)))
        .collect()
}
