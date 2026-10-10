//! Notes the user sent that no terminal took, on a page or on the agent's
//! changes: kept for the agent to fetch with `browser feedback`. A batch goes one way only. When its agent is
//! waiting in `browser feedback --wait`, the batch is handed there; otherwise
//! it is pasted into the agent's pane, and only a pane that no longer exists
//! leaves it here. Only the Unix control socket fetches or waits, so the
//! methods for it exist only where that socket does.
use super::Scope;
use gpui::Global;
use std::collections::VecDeque;

/// Batches kept at once; the oldest goes first.
const MAX_KEPT: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FeedbackKey {
    pub scope: Scope,
    pub pane_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Batch {
    /// The Herdr pane of the agent the notes are for.
    pub target: FeedbackKey,
    pub text: String,
}

#[derive(Default)]
pub(crate) struct Feedback {
    kept: VecDeque<Batch>,
    /// Panes whose agents are waiting in `browser feedback --wait`.
    waiting: Vec<FeedbackKey>,
}

impl Global for Feedback {}

impl Feedback {
    #[cfg(any(unix, test))]
    pub(crate) fn waiting(&self) -> &[FeedbackKey] {
        &self.waiting
    }

    pub(crate) fn is_waiting(&self, target: &FeedbackKey) -> bool {
        self.waiting.contains(target)
    }

    /// Records which panes are waiting.
    #[cfg(any(unix, test))]
    pub(crate) fn set_waiting(&mut self, panes: Vec<FeedbackKey>) {
        self.waiting = panes;
    }

    pub(crate) fn keep(&mut self, batch: Batch) {
        if self.kept.len() >= MAX_KEPT {
            self.kept.pop_front();
        }
        self.kept.push_back(batch);
    }

    /// Everything kept for this daemon and pane, oldest first, joined into one text.
    #[cfg(any(unix, test))]
    pub(crate) fn take(&mut self, target: &FeedbackKey) -> Option<String> {
        let (taken, kept): (Vec<_>, Vec<_>) = self
            .kept
            .drain(..)
            .partition(|batch| &batch.target == target);
        self.kept = kept.into();
        (!taken.is_empty()).then(|| {
            taken
                .into_iter()
                .map(|batch| batch.text)
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    #[cfg(any(unix, test))]
    pub(crate) fn has(&self, target: &FeedbackKey) -> bool {
        self.kept.iter().any(|batch| &batch.target == target)
    }
}

#[cfg(test)]
mod tests;
