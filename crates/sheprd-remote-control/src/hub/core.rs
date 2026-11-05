//! A running herdr-core is an independent source. This service only joins;
//! opening a phone never starts another user's terminal runtime.

mod error;
mod projection;
mod screen;
mod spaces;

use super::Error as HubError;
use super::{ApiMethod, Layout};
pub use error::Error;
use serde_json::Value;
use sheprd_core_client::{Core, wire::Terminal};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError, Weak,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tokio::sync::watch;

const POLL: Duration = Duration::from_millis(500);
const RETRY: Duration = Duration::from_secs(3);
const PREFIX: &str = "core:";

type Result<T> = std::result::Result<T, HubError>;

impl From<sheprd_core_client::Error> for HubError {
    fn from(error: sheprd_core_client::Error) -> Self {
        Self::Core(Error::Client(error))
    }
}

pub(super) struct Source {
    connected: AtomicBool,
    endpoint: crate::core_target::CoreTarget,
    scope: Option<String>,
    state: Mutex<State>,
    tabs: Option<std::path::PathBuf>,
}

#[derive(Default)]
struct State {
    core: Option<Core>,
    names: spaces::Names,
    screens: HashMap<String, screen::Screen>,
    terminals: Vec<Terminal>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Source {
    pub(super) fn start(
        layout: watch::Sender<Arc<Layout>>,
        target: &sheprd_client::ConnectTarget,
        endpoint: crate::core_target::CoreTarget,
        state_dir: &std::path::Path,
    ) -> std::io::Result<Arc<Self>> {
        let source = Arc::new(Self {
            connected: AtomicBool::new(false),
            endpoint,
            scope: target
                .socket_path()
                .ok()
                .map(|path| format!("local:{}", path.display())),
            state: Mutex::new(State::default()),
            tabs: tabs_path(state_dir),
        });
        let weak = Arc::downgrade(&source);
        thread::Builder::new()
            .name("remote-core".into())
            .spawn(move || run(weak, layout))?;
        Ok(source)
    }

    pub(super) fn connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    fn poll(&self) -> Result<Layout> {
        let mut state = lock(&self.state);
        if state.core.as_ref().is_none_or(Core::is_lost) {
            state.screens.clear();
            state.core = None;
            let endpoint = self.endpoint.endpoint()?;
            state.core = Some(Core::connect(&endpoint)?);
        }
        let core = state.core.as_ref().ok_or(Error::Disconnected)?;
        let terminals = core.list(false)?.terminals;
        let boot = core.hello().core.id.clone();
        let spaces = core
            .doc_get("herdr-app")
            .ok()
            .and_then(|doc| spaces::Spaces::read(doc.value));
        state.screens.retain(|address, screen| {
            !screen.expired()
                && terminals
                    .iter()
                    .any(|terminal| &terminal.address == address)
        });
        let next = if let Some(spaces) = spaces {
            spaces.project(&boot, &terminals, &mut state.names)
        } else {
            let saved = self.tabs.as_deref().and_then(projection::read);
            projection::project(&boot, &terminals, saved.as_ref(), self.scope.as_deref())
        };
        state.terminals = terminals;
        self.connected.store(true, Ordering::Release);
        Ok(next)
    }

    pub(super) fn call(&self, method: ApiMethod, params: &Value) -> Result<Value> {
        if !matches!(
            method,
            ApiMethod::PaneRead | ApiMethod::PaneSendKeys | ApiMethod::PaneSendText
        ) {
            return Err(Error::Unsupported.into());
        }
        let pane = params["pane_id"].as_str().ok_or(HubError::Response)?;
        let mut state = lock(&self.state);
        let terminal = state
            .terminals
            .iter()
            .find(|terminal| id(&terminal.address) == pane)
            .cloned()
            .ok_or(HubError::Response)?;
        let State { core, screens, .. } = &mut *state;
        let core = core.as_ref().ok_or(Error::Disconnected)?;
        if screens
            .get(&terminal.address)
            .is_some_and(screen::Screen::expired)
        {
            screens.remove(&terminal.address);
        }
        if !screens.contains_key(&terminal.address) {
            // Bound emulators and attachments even when a phone visits many panes.
            if screens.len() >= 16 {
                return Err(HubError::Busy);
            }
            screens.insert(
                terminal.address.clone(),
                screen::Screen::open(core, &terminal)?,
            );
        }
        let screen = screens
            .get_mut(&terminal.address)
            .ok_or(HubError::Response)?;
        screen.follow_size(&terminal)?;
        screen.call(method, params)
    }
}

fn run(source: Weak<Source>, layout: watch::Sender<Arc<Layout>>) {
    while let Some(source) = source.upgrade() {
        let (next, pause) = match source.poll() {
            Ok(next) => (next, POLL),
            Err(error) => {
                source.connected.store(false, Ordering::Release);
                tracing::debug!(%error, "remote-control could not join herdr-core");
                *lock(&source.state) = State::default();
                (Layout::default(), RETRY)
            }
        };
        layout.send_if_modified(|current| {
            if **current == next {
                return false;
            }
            *current = Arc::new(next);
            true
        });
        drop(source);
        thread::sleep(pause);
    }
}

/// Hex keeps core addresses (including remote addresses) in one URL segment.
fn id(address: &str) -> String {
    use std::fmt::Write as _;
    let mut id = PREFIX.to_owned();
    for byte in address.bytes() {
        let _ = write!(id, "{byte:02x}");
    }
    id
}

pub(super) fn owns(params: &Value) -> bool {
    ["pane_id", "tab_id", "workspace_id"].iter().any(|key| {
        params[key]
            .as_str()
            .is_some_and(|id| id.starts_with(PREFIX))
    })
}

fn merge(daemon: &Layout, core: &Layout) -> Layout {
    let mut merged = daemon.clone();
    merged.connected |= core.connected;
    if !daemon.connected {
        merged.boot = core.boot.clone();
    }
    for workspace in &core.workspaces {
        if let Some(existing) = merged
            .workspaces
            .iter_mut()
            .find(|row| row.id == workspace.id)
        {
            existing.terminals.extend(workspace.terminals.clone());
        } else {
            merged.workspaces.push(workspace.clone());
        }
    }
    merged
}

pub(super) async fn merge_sources(
    sender: watch::Sender<Arc<Layout>>,
    mut daemon: watch::Receiver<Arc<Layout>>,
    mut core: watch::Receiver<Arc<Layout>>,
) {
    loop {
        let next = merge(&daemon.borrow_and_update(), &core.borrow_and_update());
        sender.send_if_modified(|current| {
            if **current == next {
                return false;
            }
            *current = Arc::new(next);
            true
        });
        tokio::select! {
            changed = daemon.changed() => if changed.is_err() { return; },
            changed = core.changed() => if changed.is_err() { return; },
            () = sender.closed() => return,
        }
    }
}

fn tabs_path(state_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    state_dir.parent().map(|dir| dir.join("browser-tabs.json"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
