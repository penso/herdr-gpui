//! The saved-host catalog: background loads of the saved devices, the
//! serialized writes of this client's host selection, and reconciling the
//! window's endpoints and selection with what the catalog says.
use super::{Endpoint, LOCAL, SAVED_PREFIX};
use crate::{Error, HerdrWindow, Result};
use gpui::Context;
use herdr_client::{ConnectTarget, SavedHost};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

pub(crate) struct Catalog {
    development: Option<bool>,
    pending: Option<mpsc::Receiver<Result<CatalogUpdate>>>,
    next_poll: Instant,
    pub(super) desired: Option<String>,
    pub(super) initialized: bool,
    pub(super) restore_pending: bool,
    pub(super) queued_write: Option<Option<String>>,
    writing: Option<mpsc::Receiver<Result<()>>>,
}

pub(super) struct CatalogUpdate {
    pub(super) hosts: Vec<SavedHost>,
    selection: Option<Option<String>>,
}

impl Catalog {
    pub fn new(target: &ConnectTarget) -> Self {
        Self {
            development: match target {
                ConnectTarget::Socket(_) => None,
                ConnectTarget::Session { development, .. } => Some(*development),
                _ => Some(false),
            },
            pending: None,
            next_poll: Instant::now(),
            desired: None,
            initialized: false,
            restore_pending: false,
            queued_write: None,
            writing: None,
        }
    }

    pub(super) fn poll(&mut self) -> Option<Result<CatalogUpdate>> {
        let development = self.development?;
        if let Some(result) = self.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.pending = None;
            self.next_poll = Instant::now() + Duration::from_secs(2);
            return Some(result);
        }
        if self.pending.is_none() && Instant::now() >= self.next_poll {
            let (tx, rx) = mpsc::sync_channel(1);
            self.pending = Some(rx);
            let startup = !self.initialized;
            if let Err(error) = std::thread::Builder::new()
                .name("herdr-gui-catalog".into())
                .spawn(move || {
                    let result = if startup {
                        herdr_client::load_saved_host_selection(development).map(
                            |(hosts, selection)| CatalogUpdate {
                                hosts,
                                selection: Some(selection),
                            },
                        )
                    } else {
                        herdr_client::load_saved_hosts(development).map(|hosts| CatalogUpdate {
                            hosts,
                            selection: None,
                        })
                    };
                    let _ = tx.send(result.map_err(Error::from));
                })
            {
                self.pending = None;
                self.next_poll = Instant::now() + Duration::from_secs(2);
                return Some(Err(error.into()));
            }
        }
        None
    }

    pub(super) fn accept(&mut self, update: &CatalogUpdate) {
        if !self.initialized {
            self.desired = update.selection.clone().flatten();
            self.restore_pending = self.desired.is_some();
            self.initialized = true;
        }
        if self.desired.as_ref().is_some_and(|id| {
            !update
                .hosts
                .iter()
                .any(|host| host.enabled && &host.id == id)
        }) {
            self.desired = None;
            self.restore_pending = false;
        }
    }

    pub(super) fn choose(&mut self, id: &str) {
        // Also cancels an in-flight startup restore when Local is clicked.
        self.initialized = true;
        self.restore_pending = false;
        self.desired = id.strip_prefix(SAVED_PREFIX).map(str::to_owned);
        if self.development.is_some() {
            self.queued_write = Some(self.desired.clone());
        }
    }

    pub(super) fn poll_write(&mut self) -> Option<Error> {
        let development = self.development?;
        let mut error = None;
        if let Some(result) = self.writing.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.writing = None;
            error = result.err();
        }
        // Serialize this client's writes so rapid choices cannot finish backwards.
        if self.writing.is_none()
            && let Some(selected) = self.queued_write.take()
        {
            let (tx, rx) = mpsc::sync_channel(1);
            match std::thread::Builder::new()
                .name("herdr-gui-selection".into())
                .spawn(move || {
                    let _ = tx.send(
                        herdr_client::store_saved_host_selection(development, selected.as_deref())
                            .map_err(Error::from),
                    );
                }) {
                Ok(_) => self.writing = Some(rx),
                Err(e) => error = Some(e.into()),
            }
        }
        error
    }
}

impl HerdrWindow {
    pub(super) fn restore_selection(&mut self, cx: &mut Context<Self>) {
        if !self.catalog.restore_pending {
            return;
        }
        let Some(id) = self
            .catalog
            .desired
            .as_ref()
            .map(|id| format!("{SAVED_PREFIX}{id}"))
        else {
            return;
        };
        if self.endpoints.iter().any(|endpoint| {
            endpoint.id == id
                && endpoint.enabled
                && endpoint.connection.handle.is_some()
                && endpoint.live.status.is_connected()
                && endpoint.live.snapshot.is_some()
        }) {
            // One handoff attempt: activation failure may fall back to Local,
            // but must neither overwrite the preference nor loop on every tick.
            self.catalog.restore_pending = false;
            self.switch_endpoint(&id, cx);
        }
    }

    pub(crate) fn reconcile_catalog(&mut self, hosts: Vec<SavedHost>, cx: &mut Context<Self>) {
        let selected = &self.endpoints[self.selected_endpoint];
        let selected_id = selected.id.clone();
        let selected_retired = self.selected_endpoint != 0
            && !hosts.iter().any(|host| {
                format!("{SAVED_PREFIX}{}", host.id) == selected_id
                    && host.enabled
                    && !entry_changed(selected, host)
            });
        if selected_retired {
            self.switch_endpoint(LOCAL, cx);
        }
        let selected_id = self.endpoints[self.selected_endpoint].id.clone();
        let mut previous = std::mem::take(&mut self.endpoints);
        let mut next = vec![previous.remove(0)];
        for host in hosts {
            let id = format!("{SAVED_PREFIX}{}", host.id);
            let mut endpoint = if let Some(index) = previous.iter().position(|e| e.id == id) {
                previous.remove(index)
            } else {
                Endpoint::new(
                    id,
                    host.label.clone(),
                    ConnectTarget::Ssh {
                        target: host.target.clone(),
                        session: host.session.clone(),
                    },
                    host.enabled,
                )
            };
            let changed = endpoint.enabled != host.enabled || entry_changed(&endpoint, &host);
            if changed {
                endpoint.stop();
                endpoint.attempts = 0;
                endpoint.connection.target = ConnectTarget::Ssh {
                    target: host.target.clone(),
                    session: host.session.clone(),
                };
                endpoint.enabled = host.enabled;
                endpoint.detached = false;
                endpoint.retry_at = Instant::now();
            }
            endpoint.label = host.label.clone();
            endpoint.saved_host = Some(host);
            next.push(endpoint);
        }
        self.endpoints = next;
        self.selected_endpoint = self
            .endpoints
            .iter()
            .position(|e| e.id == selected_id)
            .unwrap_or(0);
        cx.notify();
    }
}

/// Whether a saved entry differs from the one this endpoint was last reconciled
/// against, which is what an edit to a device's saved profile looks like. An
/// endpoint that has never been reconciled compares its live target instead, so
/// one built outside the catalog still retires when its entry changes.
fn entry_changed(endpoint: &Endpoint, host: &SavedHost) -> bool {
    match &endpoint.saved_host {
        Some(saved) => saved.target != host.target || saved.session != host.session,
        None => !same_target(&endpoint.connection.target, host),
    }
}

/// Whether an endpoint's live target is exactly the saved entry's, session
/// included. A device's identity as the catalog describes it; the sessions list
/// deliberately points an endpoint at other sessions of the same device.
fn same_target(target: &ConnectTarget, host: &SavedHost) -> bool {
    matches!(target, ConnectTarget::Ssh { target, session } if target == &host.target && session == &host.session)
}

#[cfg(test)]
mod tests;
