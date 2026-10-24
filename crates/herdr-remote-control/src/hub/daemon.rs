//! The connection to this machine's Herdr daemon: an inactive client (no
//! surface interest, so it never sizes a pane) whose snapshots become the
//! [`Layout`] phones list. Herdr takes neither input nor endpoint requests
//! from an inactive client; those go through [`super::api`]. A worker
//! thread owns the blocking I/O; the server only reads the layout.

use super::layout::{self, Layout};
use herdr_client::{ClientEvent, ClientHandle, ConnectOptions, ConnectTarget};
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
    stop: AtomicBool,
}

impl Daemon {
    /// Starts the worker, which connects now and again whenever the daemon
    /// goes away.
    pub(crate) fn start(
        layout: watch::Sender<Arc<Layout>>,
        target: ConnectTarget,
    ) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            handle: Mutex::new(None),
            layout,
            stop: AtomicBool::new(false),
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
                stop: AtomicBool::new(true),
            }),
        }
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
    while !shared.stop.load(Ordering::Acquire) {
        if let Ok(client) = herdr_client::connect_with_surface_active(
            target.clone(),
            ConnectOptions::default(),
            false,
        ) {
            serve(shared, client);
        }
        *lock(&shared.handle) = None;
        shared.layout.send_replace(Arc::new(Layout::default()));
        thread::sleep(RETRY);
    }
}

fn serve(shared: &Shared, client: herdr_client::Client) {
    let handle = client.handle.clone();
    *lock(&shared.handle) = Some(handle.clone());
    while let Ok(event) = client.events.recv() {
        if shared.stop.load(Ordering::Acquire) {
            handle.disconnect();
            return;
        }
        match event {
            ClientEvent::Snapshot(snapshot) => {
                let next = layout::project(&snapshot);
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
