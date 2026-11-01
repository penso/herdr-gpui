//! A worker that reads Azure DevOps alone, with no checkout behind it: an
//! organization's catalog and its reader's own pull requests, or one
//! repository browsed from that catalog. It answers the same [`Service`] the
//! checkout worker does, so the view lists its pull requests with the rest,
//! but it finds no repository, reads no branches, opens no database, and
//! takes no actions: what it shows lasts the session and is read again.

use super::{
    COMMANDS, Command, Error, Notice, Result, Service, Snapshot, SourceStatus, SyncState, Timing,
    push, retry_wait,
};
use crate::{
    Item, Provider, Repository, SourceKey, azure,
    repo::{Remote, RepoInfo},
};
use chrono::Utc;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    time::{Duration, Instant},
};

/// How often a remote scope is read again: its lists are wider than one
/// checkout's, and nothing local changes them.
const SYNC: Duration = Duration::from_secs(300);

/// What a remote worker reads.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RemoteScope {
    /// The organization's projects and repositories, and every pull request
    /// its reader created there.
    Mine { organization: String },
    /// One repository's active and latest settled pull requests.
    Repository(SourceKey),
}

impl RemoteScope {
    /// The source its status is shown under: the organization, or the
    /// repository.
    pub fn source(&self) -> SourceKey {
        match self {
            Self::Mine { organization } => SourceKey {
                provider: Provider::Azure,
                host: azure::HOST.to_owned(),
                repository: organization.clone(),
            },
            Self::Repository(source) => source.clone(),
        }
    }

    /// The organization it belongs to.
    pub fn organization(&self) -> &str {
        match self {
            Self::Mine { organization } => organization,
            Self::Repository(source) => source
                .repository
                .split_once('/')
                .map_or(source.repository.as_str(), |(organization, _)| organization),
        }
    }

    /// The repository the view shows for it: no checkout, its source as
    /// the remote, so the view names it and reads its reader as Azure's.
    pub fn info(&self) -> RepoInfo {
        let source = self.source();
        RepoInfo {
            repository: Repository::Local(PathBuf::from(format!(
                "{}/{}",
                source.host, source.repository
            ))),
            main_root: String::new(),
            beads: false,
            remote: Some(Remote {
                name: "azure".into(),
                url: format!("https://{}/{}", source.host, source.repository),
                source,
            }),
        }
    }

    /// The snapshot shown before the first read ends.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            repo: Some(self.info()),
            sources: vec![SourceStatus {
                key: self.source(),
                state: SyncState::Syncing { count: None },
            }],
            ..Snapshot::default()
        }
    }
}

/// Where a remote worker reads, and how.
#[derive(Clone, Debug)]
pub struct RemoteRequest {
    pub scope: RemoteScope,
    pub timing: Timing,
    /// Whether to look the Azure DevOps token up; tests turn it off, so
    /// nothing runs `az` and every read asks to sign in instead.
    pub look_up_token: bool,
}

impl RemoteRequest {
    pub fn new(scope: RemoteScope) -> Self {
        Self {
            scope,
            timing: Timing {
                sync: SYNC,
                ..Timing::default()
            },
            look_up_token: true,
        }
    }
}

/// What one read of a scope found.
pub struct Read {
    pub items: Vec<Item>,
    pub complete: bool,
    pub viewer: Option<String>,
    pub catalog: Option<azure::organization::Catalog>,
}

/// Reads `scope` through `fetch`; the catalog only when `catalog` is set,
/// as it changes seldom and is read again on Refresh.
pub fn read(
    scope: &RemoteScope,
    catalog: bool,
    fetch: &mut impl FnMut(azure::Call) -> Result<Vec<u8>>,
) -> Result<Read> {
    match scope {
        RemoteScope::Mine { organization } => {
            let catalog = catalog
                .then(|| azure::organization::catalog(organization, fetch))
                .transpose()?;
            let mine = azure::organization::mine(organization, fetch)?;
            Ok(Read {
                items: mine.items,
                complete: mine.complete,
                viewer: mine.viewer,
                catalog,
            })
        }
        RemoteScope::Repository(source) => {
            let listing = azure::list_pull_requests(source, None, fetch)?;
            Ok(Read {
                items: listing.items,
                complete: listing.complete,
                viewer: listing.viewer,
                catalog: None,
            })
        }
    }
}

/// Shows `read` in `snapshot`, keeping a catalog it did not read again.
pub fn apply(snapshot: &mut Snapshot, read: Read) {
    let count = read.items.len();
    snapshot.items = Arc::new(read.items);
    if read.viewer.is_some() {
        snapshot.viewer = read.viewer;
    }
    if let Some(catalog) = read.catalog {
        snapshot.catalog = Some(Arc::new(catalog));
    }
    let state = if read.complete {
        SyncState::Synced {
            at: Utc::now(),
            count,
        }
    } else {
        SyncState::Capped { count }
    };
    for status in &mut snapshot.sources {
        status.state = state.clone();
    }
}

impl Service {
    /// Starts a worker reading `request`'s scope, with no checkout.
    pub fn start_remote(request: RemoteRequest) -> std::io::Result<Self> {
        let (commands, receiver) = mpsc::sync_channel(COMMANDS);
        let mailbox = Arc::new(Mutex::new(None));
        let notices = Arc::new(Mutex::new(VecDeque::new()));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker = RemoteWorker {
            snapshot: request.scope.snapshot(),
            request,
            mailbox: mailbox.clone(),
            notices: notices.clone(),
            cancelled: cancelled.clone(),
            token: None,
            cooldown: None,
            retry_until: None,
            catalog: true,
        };
        std::thread::Builder::new()
            .name("herdr-orchestrator-remote".into())
            .spawn(move || worker.run(&receiver))?;
        Ok(Self {
            commands,
            mailbox,
            notices,
            cancelled,
        })
    }
}

struct RemoteWorker {
    request: RemoteRequest,
    mailbox: Arc<Mutex<Option<Snapshot>>>,
    notices: Arc<Mutex<VecDeque<Notice>>>,
    cancelled: Arc<AtomicBool>,
    snapshot: Snapshot,
    /// The token, once looked up; Refresh and a refusal look again.
    token: Option<Option<azure::token::Token>>,
    cooldown: Option<Duration>,
    /// A failed read waits until then before the timer reads again.
    retry_until: Option<Instant>,
    /// The next read also reads the catalog.
    catalog: bool,
}

impl RemoteWorker {
    fn run(mut self, commands: &mpsc::Receiver<Command>) {
        self.publish();
        self.sync();
        let Timing { reload, sync } = self.request.timing;
        let mut next_sync = Instant::now() + sync;
        while !self.cancelled.load(Ordering::Relaxed) {
            let wait = reload.min(next_sync.saturating_duration_since(Instant::now()));
            match commands.recv_timeout(wait) {
                Ok(Command::Refresh(_)) => {
                    self.token = None;
                    self.retry_until = None;
                    self.catalog = true;
                    self.sync();
                    next_sync = Instant::now() + sync;
                }
                Ok(Command::Act(_)) => push(
                    &self.notices,
                    Notice {
                        outcome: Err(Arc::new(Error::Azure(azure::Error::NoCheckout {
                            repository: self.request.scope.source().repository,
                        }))),
                    },
                ),
                // Nothing here has a GitHub pull request, agents, or actions.
                Ok(Command::Acted { .. } | Command::LoadPullRequest(_)) => {}
                Ok(Command::LoadAgents { .. }) => {}
                Err(RecvTimeoutError::Timeout) if Instant::now() >= next_sync => {
                    if self.retry_until.is_none_or(|until| Instant::now() >= until) {
                        self.sync();
                    }
                    next_sync = Instant::now() + sync;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn publish(&self) {
        if let Ok(mut mailbox) = self.mailbox.lock() {
            *mailbox = Some(self.snapshot.clone());
        }
    }

    fn sync(&mut self) {
        for status in &mut self.snapshot.sources {
            status.state = SyncState::Syncing {
                count: status.state.count(),
            };
        }
        self.publish();
        self.cooldown = None;
        let looked_up = self.token.is_some();
        let mut result = self.read();
        if looked_up && matches!(result, Err(Error::Azure(azure::Error::Authentication))) {
            // An Azure CLI token lasts about an hour; the next comes from
            // the same sign-in.
            self.token = None;
            result = self.read();
        }
        match result {
            Ok(read) => {
                self.catalog = false;
                self.retry_until = None;
                apply(&mut self.snapshot, read);
            }
            Err(Error::Cancelled) => return,
            Err(error) => {
                let wait = retry_wait(&error, self.cooldown.take());
                tracing::warn!(
                    category = "orchestrator_sync",
                    scope = ?self.request.scope,
                    retry_in_secs = wait.as_secs(),
                    error = %error,
                    "Azure DevOps read failed"
                );
                self.retry_until = Some(Instant::now() + wait);
                let error = Arc::new(error);
                for status in &mut self.snapshot.sources {
                    status.state = SyncState::Failed(error.clone());
                }
            }
        }
        self.publish();
    }

    fn read(&mut self) -> Result<Read> {
        if self.token.is_none() {
            let cancelled = &self.cancelled;
            self.token = Some(if self.request.look_up_token {
                azure::token::lookup(&|| cancelled.load(Ordering::Relaxed))
            } else {
                None
            });
        }
        let token = self.token.as_ref().and_then(Option::as_ref);
        let cancelled = &self.cancelled;
        let scope = &self.request.scope;
        let catalog = self.catalog;
        azure::over_https(
            &scope.source().repository,
            token,
            &|| cancelled.load(Ordering::Relaxed),
            &mut self.cooldown,
            |mut fetch| read(scope, catalog, &mut fetch),
        )
    }
}
