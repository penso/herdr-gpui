//! The named Herdr 1 sessions of this user's installation beside the primary
//! one at `--herdr-socket`: one inactive client and API socket each, so a
//! computer that runs `herdr --session NAME` shows phones every session, not
//! only the one Sheprd enrolled. A session's workspace, tab, and pane ids carry
//! `herdr:NAME:`, as a core's carry `core:`; session names cannot hold `:`.
//! The primary keeps its ids, its boot, and everything else it had.

use super::{
    Error, Result,
    api::Api,
    daemon::Daemon,
    layout::{Layout, MAX_CHATS, Workspace},
};
use serde_json::Value;
use sheprd_client::{ConnectTarget, LocalSession, SessionState};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
    time::Duration,
};
use tokio::sync::{Notify, watch};

const PREFIX: &str = "herdr:";
/// The request and answer fields that name a workspace, tab, or pane.
pub(super) const ID_KEYS: [&str; 4] = ["pane_id", "tab_id", "target_pane_id", "workspace_id"];
/// How often sessions are listed again, so one started or stopped after this
/// daemon is followed or let go without a restart.
const RESCAN: Duration = Duration::from_secs(30);

pub(super) struct Sessions {
    /// The primary's client socket, which is never followed twice.
    primary: Option<PathBuf>,
    list: Mutex<Vec<Arc<Session>>>,
    /// Wakes the merge when a session's layout changes or the list does.
    changed: Notify,
}

pub(super) struct Session {
    pub(super) name: String,
    pub(super) api: Api,
    layout: watch::Receiver<Arc<Layout>>,
    /// Owns the worker; dropping it disconnects. `None` in tests.
    _daemon: Option<Daemon>,
}

impl Sessions {
    /// Follows the running sessions now and every [`RESCAN`]. Must run
    /// inside a Tokio runtime.
    pub(super) fn start(primary: Option<PathBuf>) -> Arc<Self> {
        let sessions = Arc::new(Self::new(primary));
        tokio::spawn(rescan(Arc::downgrade(&sessions)));
        sessions
    }

    fn new(primary: Option<PathBuf>) -> Self {
        Self {
            primary,
            list: Mutex::new(Vec::new()),
            changed: Notify::new(),
        }
    }

    /// No sessions, and none looked for.
    #[cfg(test)]
    pub(super) fn offline() -> Arc<Self> {
        Arc::new(Self::new(None))
    }

    /// One session answering with `layout` at `client_socket`'s API socket.
    #[cfg(all(test, unix))]
    pub(super) fn offline_with(name: &str, layout: Layout, client_socket: PathBuf) -> Arc<Self> {
        let sessions = Self::new(None);
        let (_, receiver) = watch::channel(Arc::new(layout));
        lock(&sessions.list).push(Arc::new(Session {
            name: name.to_owned(),
            api: Api::new(Some(client_socket)),
            layout: receiver,
            _daemon: None,
        }));
        Arc::new(sessions)
    }

    pub(super) fn get(&self, name: &str) -> Option<Arc<Session>> {
        lock(&self.list)
            .iter()
            .find(|session| session.name == name)
            .cloned()
    }

    /// The first session that answers, for work no id routes.
    pub(super) fn first_connected(&self) -> Option<Arc<Session>> {
        lock(&self.list)
            .iter()
            .find(|session| session.layout.borrow().connected)
            .cloned()
    }

    fn layouts(&self) -> Vec<(String, Arc<Layout>)> {
        lock(&self.list)
            .iter()
            .map(|session| (session.name.clone(), session.layout.borrow().clone()))
            .collect()
    }

    /// Follows the sessions in `wanted`, and lets go of the rest unless they
    /// still answer: one probe that missed is no reason to drop a session.
    /// Workers start and stop outside the lock phones' requests wait on.
    /// Only the rescan task calls it, so the list cannot change in between.
    pub(super) fn update(self: &Arc<Self>, wanted: &[(String, PathBuf)]) {
        let known: Vec<String> = lock(&self.list)
            .iter()
            .map(|session| session.name.clone())
            .collect();
        let mut added = Vec::new();
        for (name, socket) in wanted {
            if known.contains(name) {
                continue;
            }
            match self.follow(name, socket.clone()) {
                Ok(session) => added.push(Arc::new(session)),
                Err(error) => {
                    tracing::warn!(session = %name, %error, "could not follow a Herdr session")
                }
            }
        }
        let changed = !added.is_empty();
        let gone = {
            let mut list = lock(&self.list);
            let (kept, gone): (Vec<_>, Vec<_>) = list.drain(..).partition(|session| {
                wanted.iter().any(|(name, _)| *name == session.name)
                    || session.layout.borrow().connected
            });
            *list = kept;
            list.extend(added);
            list.sort_by(|a, b| a.name.cmp(&b.name));
            gone
        };
        if changed || !gone.is_empty() {
            self.changed.notify_one();
        }
        drop(gone);
    }

    fn follow(self: &Arc<Self>, name: &str, socket: PathBuf) -> std::io::Result<Session> {
        let (sender, layout) = watch::channel(Arc::new(Layout::default()));
        let daemon = Daemon::start(sender, ConnectTarget::Socket(socket.clone()))?;
        let mut changes = layout.clone();
        let sessions = Arc::downgrade(self);
        tokio::spawn(async move {
            // Ends when the worker drops its sender, once its session goes.
            while changes.changed().await.is_ok() {
                let Some(sessions) = sessions.upgrade() else {
                    return;
                };
                sessions.changed.notify_one();
            }
        });
        Ok(Session {
            name: name.to_owned(),
            api: Api::new(Some(socket)),
            layout,
            _daemon: Some(daemon),
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

async fn rescan(sessions: Weak<Sessions>) {
    let mut every = tokio::time::interval(RESCAN);
    every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        every.tick().await;
        let Some(current) = sessions.upgrade() else {
            return;
        };
        let primary = current.primary.clone();
        // Probes each session's socket: blocking, off the runtime's threads.
        match tokio::task::spawn_blocking(move || listed(primary.as_deref())).await {
            Ok(Ok(wanted)) => current.update(&wanted),
            Ok(Err(error)) => tracing::warn!(%error, "could not list Herdr sessions"),
            Err(error) => tracing::warn!(%error, "the Herdr session listing stopped"),
        }
    }
}

/// The sessions to follow, listed in the installation the primary's socket
/// is in: it was resolved in the user's shell, whose `XDG_CONFIG_HOME` this
/// service's environment may lack. Blocking.
pub(super) fn listed(primary: Option<&Path>) -> sheprd_client::Result<Vec<(String, PathBuf)>> {
    listed_with(primary, || sheprd_client::list_local_sessions(false))
}

/// [`listed`], with `own` listing this process's installation. Only a primary
/// that could not be resolved falls back to it: a socket outside any
/// installation, such as a sandbox's or a development daemon's, was chosen to
/// keep this daemon to that session, so nothing else is followed.
pub(super) fn listed_with(
    primary: Option<&Path>,
    own: impl FnOnce() -> sheprd_client::Result<Vec<LocalSession>>,
) -> sheprd_client::Result<Vec<(String, PathBuf)>> {
    let listed = match primary {
        Some(primary) => match installation(primary) {
            Some(dir) => sheprd_client::list_running_sessions_in(dir)?,
            None => return Ok(Vec::new()),
        },
        None => own()?,
    };
    Ok(wanted(listed, primary))
}

/// The release installation a session's client socket is in: `DIR` of
/// `DIR/herdr-client.sock` or `DIR/sessions/NAME/herdr-client.sock`, where
/// `DIR` is named `herdr`.
fn installation(socket: &Path) -> Option<&Path> {
    if socket.file_name()? != "herdr-client.sock" {
        return None;
    }
    let dir = socket.parent()?;
    let dir = match dir.parent() {
        Some(sessions) if sessions.file_name().is_some_and(|name| name == "sessions") => {
            sessions.parent()?
        }
        _ => dir,
    };
    (dir.file_name()? == "herdr").then_some(dir)
}

/// The sessions to follow: those running, except the primary's, whose names
/// `herdr:NAME:` can carry.
pub(super) fn wanted(listed: Vec<LocalSession>, primary: Option<&Path>) -> Vec<(String, PathBuf)> {
    listed
        .into_iter()
        .filter(|session| {
            session.state == SessionState::Running
                && !session.name.is_empty()
                && !session.name.contains(':')
                && !primary.is_some_and(|primary| same_socket(primary, &session.socket))
        })
        .map(|session| (session.name, session.socket))
        .collect()
}

/// Whether two paths name one socket, also through a symlink such as macOS's
/// `/tmp` for `/private/tmp`.
fn same_socket(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// Joins the sessions' layouts to `base` (the primary's and the core's) as
/// they change, until no one reads the merged layout.
pub(super) async fn merge_sources(
    sender: watch::Sender<Arc<Layout>>,
    mut base: watch::Receiver<Arc<Layout>>,
    sessions: Arc<Sessions>,
) {
    loop {
        let next = {
            let base = base.borrow_and_update().clone();
            merge(&base, &sessions.layouts())
        };
        sender.send_if_modified(|current| {
            if **current == next {
                return false;
            }
            *current = Arc::new(next);
            true
        });
        tokio::select! {
            changed = base.changed() => if changed.is_err() { return; },
            () = sessions.changed.notified() => {}
            () = sender.closed() => return,
        }
    }
}

/// `base` followed by each session's workspaces under its prefix. The boot
/// stays the base's: the desktop's pull request data is keyed on it. Sessions
/// share what [`MAX_CHATS`] leaves after the base, in order; the chats past
/// that are left out.
pub(super) fn merge(base: &Layout, sessions: &[(String, Arc<Layout>)]) -> Layout {
    let mut merged = base.clone();
    let mut room = MAX_CHATS.saturating_sub(base.chats().count());
    for (name, layout) in sessions {
        let mut layout = (**layout).clone();
        prefix_layout(&mut layout, name);
        for workspace in &mut layout.workspaces {
            workspace.chats.truncate(room);
            room -= workspace.chats.len();
        }
        merged.connected |= layout.connected;
        merged.working += layout.working;
        merged.workspaces.append(&mut layout.workspaces);
    }
    merged
}

fn tag(name: &str, id: &str) -> String {
    format!("{PREFIX}{name}:{id}")
}

/// Puts `name`'s prefix on every id `layout` lists.
pub(super) fn prefix_layout(layout: &mut Layout, name: &str) {
    for Workspace {
        id,
        chats,
        terminals,
        ..
    } in &mut layout.workspaces
    {
        *id = tag(name, id);
        for chat in chats {
            chat.pane = tag(name, &chat.pane);
            chat.tab = tag(name, &chat.tab);
        }
        for terminal in terminals {
            terminal.tab = tag(name, &terminal.tab);
            for pane in &mut terminal.panes {
                pane.pane = tag(name, &pane.pane);
            }
            if let Some(agent) = &mut terminal.agent {
                agent.pane = tag(name, &agent.pane);
            }
        }
    }
}

/// The session a request's ids name, with the request as that session knows
/// it; `None` when they name the primary's (or nothing). A request naming two
/// sessions, or the primary and a session, is refused.
pub(super) fn owner(params: &Value) -> Result<Option<(String, Value)>> {
    let mut owner: Option<&str> = None;
    let mut primary = false;
    let mut stripped = params.clone();
    for key in ID_KEYS {
        let Some(id) = params[key].as_str() else {
            continue;
        };
        let Some(rest) = id.strip_prefix(PREFIX) else {
            primary = true;
            continue;
        };
        let (name, id) = rest
            .split_once(':')
            .filter(|(name, id)| !name.is_empty() && !id.is_empty())
            .ok_or(Error::Response)?;
        if owner.is_some_and(|owner| owner != name) {
            return Err(Error::Response);
        }
        owner = Some(name);
        stripped[key] = Value::String(id.to_owned());
    }
    match owner {
        Some(_) if primary => Err(Error::Response),
        Some(name) => Ok(Some((name.to_owned(), stripped))),
        None => Ok(None),
    }
}

/// Puts `name`'s prefix on the ids in a session's answer, however deep, so a
/// phone's next request for them comes back to that session.
pub(super) fn prefix_answer(answer: &mut Value, name: &str) {
    match answer {
        Value::Object(fields) => {
            for (key, value) in fields.iter_mut() {
                match value {
                    Value::String(id) if !id.is_empty() && ID_KEYS.contains(&key.as_str()) => {
                        *id = tag(name, id);
                    }
                    _ => prefix_answer(value, name),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                prefix_answer(item, name);
            }
        }
        _ => {}
    }
}
