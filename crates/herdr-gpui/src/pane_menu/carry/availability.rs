//! Bounded, cancellable eligibility query for non-repository panes. Only the
//! carry worker reads sessions and home directories, never render or input.

use super::*;
use crate::i18n::fl;
use herdr_client::protocol::{ClientShellAgent, ClientShellPane};

/// What the window's snapshot last showed of the pane: its agent entry, whose
/// `state_change_seq` advances on every state change, and its directories.
/// A refusal made under one state is not trusted under another.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::pane_menu) struct PaneState {
    agent: Option<ClientShellAgent>,
    pane: Option<(Option<String>, Option<String>)>,
}

impl PaneState {
    pub(in crate::pane_menu) fn of(snapshot: &ClientShellSnapshot, pane_id: &str) -> Self {
        Self {
            agent: snapshot
                .agents
                .iter()
                .find(|agent| agent.pane_id == pane_id)
                .cloned(),
            pane: snapshot
                .panes
                .iter()
                .find(|pane| pane.pane_id == pane_id)
                .map(
                    |ClientShellPane {
                         cwd,
                         foreground_cwd,
                         ..
                     }| { (cwd.clone(), foreground_cwd.clone()) },
                ),
        }
    }
}

pub(in crate::pane_menu) struct Availability {
    cancelled: Arc<AtomicBool>,
    receiver: mpsc::Receiver<carry::Result<()>>,
    result: Option<carry::Result<()>>,
    seen: PaneState,
}

impl Availability {
    pub(in crate::pane_menu) fn check(origin: Origin, seen: PaneState) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("herdr-carry-availability".into())
            .spawn(move || {
                let _ = sender.send(carry::check_session(&origin, &flag));
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "could not check pane move availability");
        }
        Self {
            cancelled,
            receiver,
            result: None,
            seen,
        }
    }

    pub(in crate::pane_menu) fn poll(&mut self) -> bool {
        if self.result.is_some() {
            return false;
        }
        self.result = match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err(crate::teleport::Error::WorkerStopped))
            }
        };
        true
    }

    /// The check refused while the pane looked different from `now`, e.g.
    /// before its agent reported a session, so it must be asked again.
    pub(in crate::pane_menu) fn stale(&self, now: &PaneState) -> bool {
        matches!(self.result, Some(Err(_))) && self.seen != *now
    }

    pub(in crate::pane_menu) fn muted(&self) -> bool {
        !matches!(self.result, Some(Ok(())))
    }

    pub(in crate::pane_menu) fn reason(&self) -> Option<String> {
        match &self.result {
            None => Some(fl!("pane-menu-carry-checking")),
            Some(Err(error)) => Some(error.to_string()),
            Some(Ok(())) => None,
        }
    }
}

impl Drop for Availability {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests;
