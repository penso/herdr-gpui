//! The saved-host catalog: background loads of the saved devices and the
//! serialized writes of this client's host selection.
use super::SAVED_PREFIX;
use crate::{Error, Result};
use herdr_client::{ConnectTarget, SavedHost};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

pub(crate) struct Catalog {
    pub(super) development: Option<bool>,
    pub(super) pending: Option<mpsc::Receiver<Result<CatalogUpdate>>>,
    pub(super) next_poll: Instant,
    pub(super) desired: Option<String>,
    pub(super) initialized: bool,
    pub(super) restore_pending: bool,
    pub(super) queued_write: Option<Option<String>>,
    pub(super) writing: Option<mpsc::Receiver<Result<()>>>,
}

pub(super) struct CatalogUpdate {
    pub(super) hosts: Vec<SavedHost>,
    pub(super) selection: Option<Option<String>>,
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
