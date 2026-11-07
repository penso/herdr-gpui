//! The connection to this machine's Herdr daemon: an inactive client (no
//! surface interest, so it never sizes a pane) whose snapshots become the
//! [`Layout`] phones list. Herdr takes neither input nor endpoint requests
//! from an inactive client; those go through [`super::api`]. A worker
//! thread owns the blocking I/O; the server only reads the layout.

use super::{
    layout::{self, Chats, Layout},
    recency::AgentTimes,
};
use sheprd_client::{ClientEvent, ClientHandle, ConnectOptions, ConnectTarget};
use std::{
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tokio::sync::watch;

/// How long the worker waits before connecting again.
const RETRY: Duration = Duration::from_secs(3);

pub(crate) struct Daemon {
    shared: Arc<Shared>,
}

struct Shared {
    /// The live connection's handle, to disconnect on drop.
    handle: Mutex<Option<ClientHandle>>,
    layout: watch::Sender<Arc<Layout>>,
    /// The Herdr 1 session its workspaces are in, when its name is known.
    session: Option<String>,
    stop: AtomicBool,
    /// The worker returned; only a WSL session's does on its own.
    ended: AtomicBool,
}

impl Daemon {
    /// Starts the worker, which connects now and again whenever the daemon
    /// goes away, and names `session` on every workspace it lists. A WSL
    /// session is connected to once: its bridge starts a server that is down
    /// and boots a stopped distribution, so what the user stopped would come
    /// straight back. The session listing decides when it is worth following
    /// again.
    pub(crate) fn start(
        layout: watch::Sender<Arc<Layout>>,
        target: ConnectTarget,
        session: Option<String>,
    ) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            handle: Mutex::new(None),
            layout,
            session,
            stop: AtomicBool::new(false),
            ended: AtomicBool::new(false),
        });
        let worker = shared.clone();
        thread::Builder::new()
            .name("herdr-remote-daemon".into())
            .spawn(move || run(&worker, &target))?;
        Ok(Self { shared })
    }

    /// A daemon that never connects, for tests that must not reach a real
    /// Herdr.
    #[cfg(test)]
    pub(crate) fn offline(layout: watch::Sender<Arc<Layout>>) -> Self {
        Self {
            shared: Arc::new(Shared {
                handle: Mutex::new(None),
                layout,
                session: None,
                stop: AtomicBool::new(true),
                ended: AtomicBool::new(false),
            }),
        }
    }

    /// Whether the worker has stopped for good.
    pub(crate) fn ended(&self) -> bool {
        self.shared.ended.load(Ordering::Acquire)
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(handle) = lock(&self.shared.handle).take() {
            handle.disconnect();
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn run(shared: &Shared, target: &ConnectTarget) {
    // Outlives a connection: a reconnect to the same boot keeps the times.
    let mut times = AgentTimes::default();
    let wsl = matches!(target, ConnectTarget::Wsl { .. });
    while !shared.stop.load(Ordering::Acquire) {
        if let Ok(client) = sheprd_client::connect_with_surface_active(
            target.clone(),
            ConnectOptions::default(),
            false,
        ) {
            let chats = if wsl { Chats::Terminals } else { Chats::Linked };
            serve(shared, client, &mut times, chats);
        }
        *lock(&shared.handle) = None;
        shared.layout.send_replace(Arc::new(Layout::default()));
        if wsl {
            break;
        }
        thread::sleep(RETRY);
    }
    shared.ended.store(true, Ordering::Release);
}

fn serve(shared: &Shared, client: sheprd_client::Client, times: &mut AgentTimes, chats: Chats) {
    let handle = client.handle.clone();
    *lock(&shared.handle) = Some(handle.clone());
    while let Ok(event) = client.events.recv() {
        if shared.stop.load(Ordering::Acquire) {
            handle.disconnect();
            return;
        }
        match event {
            ClientEvent::Snapshot(snapshot) => {
                let mut next = layout::project_with(&snapshot, chats);
                if let Some(session) = &shared.session {
                    next.name_session(session);
                }
                times.stamp(&snapshot, &mut next, crate::web::now_ms());
                shared.layout.send_if_modified(|current| {
                    let changed = **current != next;
                    if changed {
                        *current = Arc::new(next);
                    }
                    changed
                });
            }
            ClientEvent::Disconnected { .. } => return,
            _ => {}
        }
    }
}
