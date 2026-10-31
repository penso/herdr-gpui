//! What herdr-core reports about each terminal: its agent's status, its
//! folder, and its name, for the chrome of split panes. Read on a short-lived
//! thread every few seconds while a split is shown, never on the UI thread.

use herdr_backend::{Backend as _, TerminalId, TerminalInfo};
use herdr_core_client::CoreBackend;
use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, TryRecvError, sync_channel},
    time::{Duration, Instant},
};

/// How often the list is read again.
const EVERY: Duration = Duration::from_secs(2);

type Listed = Receiver<Result<Vec<TerminalInfo>, herdr_core_client::Error>>;

#[derive(Default)]
pub(super) struct Infos {
    known: HashMap<TerminalId, TerminalInfo>,
    last: Option<Instant>,
    pending: Option<Listed>,
}

impl Infos {
    pub(super) fn get(&self, id: &TerminalId) -> Option<&TerminalInfo> {
        self.known.get(id)
    }

    /// Takes a finished read and starts the next when one is due. True when
    /// what is known changed.
    pub(super) fn poll(&mut self, backend: Option<&CoreBackend>, now: Instant) -> bool {
        let mut changed = false;
        if let Some(pending) = &self.pending {
            match pending.try_recv() {
                Ok(Ok(list)) => {
                    let known: HashMap<TerminalId, TerminalInfo> = list
                        .into_iter()
                        .map(|info| (info.id.clone(), info))
                        .collect();
                    changed = known != self.known;
                    self.known = known;
                    self.pending = None;
                }
                Ok(Err(error)) => {
                    tracing::debug!(%error, "Could not list herdr-core terminals");
                    self.pending = None;
                }
                Err(TryRecvError::Disconnected) => self.pending = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        let due = self
            .last
            .is_none_or(|last| now.duration_since(last) >= EVERY);
        if let Some(backend) = backend.filter(|_| self.pending.is_none() && due) {
            self.last = Some(now);
            let backend = backend.clone();
            let (send, receive) = sync_channel(1);
            let started = std::thread::Builder::new()
                .name("herdr-core-list".into())
                .spawn(move || {
                    let _ = send.send(backend.terminals());
                });
            match started {
                Ok(_) => self.pending = Some(receive),
                Err(error) => tracing::warn!(%error, "Could not start a herdr-core list worker"),
            }
        }
        changed
    }
}
