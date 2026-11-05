//! A running herdr-core is an independent source. This service only joins;
//! opening a phone never starts another user's terminal runtime.

mod chats;
mod create;
mod error;
mod poller;
mod projection;
mod screen;
mod spaces;

use super::Error as HubError;
use super::{ApiMethod, Layout};
pub use error::Error;
use serde_json::Value;
use sheprd_core_client::{Core, Subscription, event::Notice, wire::Terminal};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError, Weak,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::watch;

const POLL: Duration = Duration::from_millis(500);
/// The longest between polls while events flow: how late a change to the
/// desktop's spaces can reach a phone.
const HEARTBEAT: Duration = Duration::from_secs(3);
/// How long a screen goes unread before a new one may take its place.
const IDLE_SCREEN: Duration = Duration::from_secs(5);
/// Bounds emulators and attachments even when a phone visits many panes.
const MAX_SCREENS: usize = 16;
const RETRY: Duration = Duration::from_secs(3);
const PREFIX: &str = "core:";
/// Where terminals nothing else lists go, new ones from a phone included.
pub(super) const UNLISTED: &str = "core:terminals";

type Result<T> = std::result::Result<T, HubError>;

impl From<sheprd_core_client::Error> for HubError {
    fn from(error: sheprd_core_client::Error) -> Self {
        Self::Core(Error::Client(error))
    }
}

pub(super) struct Source {
    connected: AtomicBool,
    endpoint: crate::core_target::CoreTarget,
    /// Only the worker reads the core's document and the saved tabs; it
    /// keeps them apart from `state`, whose lock phones wait on.
    poller: Mutex<poller::Poller>,
    scope: Option<String>,
    state: Mutex<State>,
    tabs: Option<std::path::PathBuf>,
}

/// What phones and the worker share. Its lock is held only to read or swap
/// these, never across a core call: a stalled core or pane must not stop
/// the other panes. Each screen has a lock of its own for the same reason.
#[derive(Default)]
struct State {
    core: Option<Arc<Core>>,
    screens: HashMap<String, Arc<Mutex<screen::Screen>>>,
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
            poller: Mutex::default(),
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

    /// The connection, made again when it was lost.
    fn connection(&self) -> Result<Arc<Core>> {
        {
            let mut state = lock(&self.state);
            if let Some(core) = state.core.clone().filter(|core| !core.is_lost()) {
                return Ok(core);
            }
            state.screens.clear();
            state.core = None;
        }
        let endpoint = self.endpoint.endpoint()?;
        let core = Arc::new(Core::connect(&endpoint)?);
        lock(&self.state).core = Some(core.clone());
        Ok(core)
    }

    fn poll(&self) -> Result<Layout> {
        let core = self.connection()?;
        let terminals = core.list(false)?.terminals;
        let boot = core.hello().core.id.clone();
        let next = {
            let mut poller = lock(&self.poller);
            poller.read_spaces(core.doc_get("herdr-app"))?;
            let mut next = poller.project(
                &boot,
                &terminals,
                self.tabs.as_deref(),
                self.scope.as_deref(),
            );
            chats::lift(&mut next, &boot, &terminals);
            next
        };
        let mut state = lock(&self.state);
        // A screen in use is not idle, so it can wait for the next poll.
        state.screens.retain(|address, screen| {
            screen.try_lock().map_or(true, |screen| !screen.expired())
                && terminals
                    .iter()
                    .any(|terminal| &terminal.address == address)
        });
        state.terminals = terminals;
        drop(state);
        self.connected.store(true, Ordering::Release);
        Ok(next)
    }

    pub(super) fn call(&self, method: ApiMethod, params: &Value) -> Result<Value> {
        let target = match method {
            ApiMethod::PaneClose => Some("pane_id"),
            ApiMethod::TabClose => Some("tab_id"),
            _ => None,
        };
        if let Some(target) = target {
            return self.close(params[target].as_str().ok_or(HubError::Response)?);
        }
        if !matches!(
            method,
            ApiMethod::PaneRead | ApiMethod::PaneSendKeys | ApiMethod::PaneSendText
        ) {
            return Err(Error::Unsupported.into());
        }
        let pane = params["pane_id"].as_str().ok_or(HubError::Response)?;
        let (core, terminal) = {
            let state = lock(&self.state);
            let core = state.core.clone().ok_or(Error::Disconnected)?;
            let boot = &core.hello().core.id;
            let terminal = state
                .terminals
                .iter()
                .find(|terminal| pane_id(boot, &terminal.address) == pane)
                .cloned()
                .ok_or(HubError::Response)?;
            (core, terminal)
        };
        let screen = self.screen(&core, &terminal)?;
        let mut screen = lock(&screen);
        screen.follow_size(&terminal)?;
        screen.call(&core, method, params)
    }

    /// The shared screen of `terminal`, opened outside the state lock.
    fn screen(&self, core: &Arc<Core>, terminal: &Terminal) -> Result<Arc<Mutex<screen::Screen>>> {
        let address = &terminal.address;
        let found = lock(&self.state).screens.get(address).cloned();
        if let Some(screen) = found {
            if !lock(&screen).expired() {
                return Ok(screen);
            }
            let mut state = lock(&self.state);
            if state
                .screens
                .get(address)
                .is_some_and(|current| Arc::ptr_eq(current, &screen))
            {
                state.screens.remove(address);
            }
        }
        let opened = screen::Screen::open(core, terminal)?;
        let mut state = lock(&self.state);
        // A reconnect while it opened leaves it on a dead connection.
        if !state
            .core
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, core))
        {
            return Err(Error::Disconnected.into());
        }
        // Another request opened it meanwhile: that one is shared.
        if let Some(screen) = state.screens.get(address) {
            return Ok(screen.clone());
        }
        if state.screens.len() >= MAX_SCREENS {
            let oldest = oldest_idle(&state.screens).ok_or(HubError::Busy)?;
            state.screens.remove(&oldest);
        }
        let screen = Arc::new(Mutex::new(opened));
        state.screens.insert(address.clone(), screen.clone());
        Ok(screen)
    }
}

/// The screen nobody has read or typed into for longest, if any is idle.
/// Bounds emulators and attachments without letting abandoned panes keep
/// out a pane a phone wants now.
fn oldest_idle(screens: &HashMap<String, Arc<Mutex<screen::Screen>>>) -> Option<String> {
    screens
        .iter()
        .filter_map(|(address, screen)| {
            let idle = screen.try_lock().ok()?.idle();
            (idle >= IDLE_SCREEN).then_some((idle, address))
        })
        .max_by_key(|(idle, _)| *idle)
        .map(|(_, address)| address.clone())
}

fn run(source: Weak<Source>, layout: watch::Sender<Arc<Layout>>) {
    let mut events = None;
    while let Some(source) = source.upgrade() {
        let started = Instant::now();
        let polled = source.poll();
        let joined = polled.is_ok();
        let next = polled.unwrap_or_else(|error| {
            source.connected.store(false, Ordering::Release);
            tracing::debug!(%error, "remote-control could not join herdr-core");
            *lock(&source.state) = State::default();
            Layout::default()
        });
        layout.send_if_modified(|current| {
            if **current == next {
                return false;
            }
            *current = Arc::new(next);
            true
        });
        if !joined {
            events = None;
            drop(source);
            thread::sleep(RETRY);
            continue;
        }
        if events.is_none() {
            let core = lock(&source.state).core.clone();
            events = core.and_then(|core| core.subscribe().ok());
        }
        drop(source);
        // Busy terminals report often: this keeps polls at most this close.
        thread::sleep(POLL.saturating_sub(started.elapsed()));
        wait(&mut events, HEARTBEAT);
    }
}

/// Waits for a terminal to change, or `limit` for what no event reports,
/// which is the desktop's document. Polling is what keeps chats linked and
/// alerts flowing with no phone open, so it slows down but never stops.
/// Without events the next poll comes right away.
fn wait(events: &mut Option<Subscription>, limit: Duration) {
    let Some(subscription) = events else {
        return;
    };
    let deadline = Instant::now() + limit;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        match subscription.next(left) {
            Ok(notices)
                if notices
                    .iter()
                    .any(|notice| !matches!(notice, Notice::Heartbeat { .. })) =>
            {
                return;
            }
            Ok(_) => {}
            Err(_) => {
                *events = None;
                return;
            }
        }
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

/// A pane's id. Addresses survive a core restart but start over when its
/// state is wiped, which also gives the core a new id: naming the core
/// keeps an open phone view from landing on an unrelated new terminal.
fn pane_id(boot: &str, address: &str) -> String {
    let tag = boot.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    format!("{PREFIX}{tag:08x}-{}", &id(address)[PREFIX.len()..])
}

pub(super) fn owns(params: &Value) -> bool {
    ["pane_id", "tab_id", "target_pane_id", "workspace_id"]
        .iter()
        .any(|key| {
            params[key]
                .as_str()
                .is_some_and(|id| id.starts_with(PREFIX))
        })
}

fn merge(daemon: &Layout, core: &Layout) -> Layout {
    let mut merged = daemon.clone();
    merged.connected |= core.connected;
    merged.working += core.working;
    if !daemon.connected {
        merged.boot = core.boot.clone();
    }
    for workspace in &core.workspaces {
        if let Some(existing) = merged
            .workspaces
            .iter_mut()
            .find(|row| row.id == workspace.id)
        {
            existing.chats.extend(workspace.chats.clone());
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
