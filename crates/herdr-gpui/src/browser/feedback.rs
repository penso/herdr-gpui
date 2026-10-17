//! Notes the user sent that no terminal took, on a page or on the agent's
//! changes: kept for the agent to fetch with `browser feedback`. A batch goes one way only. When its agent is
//! waiting in `browser feedback --wait`, the batch is handed there; otherwise
//! it is pasted into the agent's pane, and only a pane that no longer exists
//! leaves it here. Only the Unix control socket fetches or waits, so the
//! methods for it exist only where that socket does.
use super::Scope;
use gpui::Global;
use std::{
    collections::VecDeque,
    sync::{Arc, Weak},
};

/// Batches kept at once; the oldest goes first.
const MAX_KEPT: usize = 16;
/// Text one `take` hands out, measured as JSON, so the answer that carries
/// it fits the control socket's message bound. Batches past it stay kept for
/// the next take.
pub(crate) const TAKE_BUDGET: usize = 240 * 1024;

/// `text` cut, at a character, to fit `budget` as JSON, saying so: a batch
/// over the budget could never be handed out in one answer.
pub(crate) fn fit(text: String, budget: usize) -> String {
    const CUT: &str = "\n[Notes cut: too long for one answer]";
    if json_size(&text) <= budget {
        return text;
    }
    let room = budget.saturating_sub(json_size(CUT));
    // Quotes, then each character as JSON escapes it.
    let mut used = 2;
    let end = text
        .char_indices()
        .find(|(_, c)| {
            used += json_size(c.encode_utf8(&mut [0; 4])) - 2;
            used > room
        })
        .map_or(text.len(), |(at, _)| at);
    format!("{}{CUT}", &text[..end])
}

/// `text`'s size inside a JSON string.
pub(crate) fn json_size(text: &str) -> usize {
    serde_json::to_string(text).map_or(text.len() * 6, |json| json.len())
}

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
    /// Each batch with its pending marker and its number in `added` order.
    kept: VecDeque<(Batch, Option<Arc<()>>, u64)>,
    /// Batches kept so far, which numbers the next one.
    added: u64,
    /// Panes whose agents are waiting in `browser feedback --wait`.
    waiting: Vec<FeedbackKey>,
}

impl Global for Feedback {}

impl Feedback {
    pub(crate) fn waiting(&self) -> &[FeedbackKey] {
        &self.waiting
    }

    pub(crate) fn is_waiting(&self, target: &FeedbackKey) -> bool {
        self.waiting.contains(target)
    }

    /// Records which panes are waiting.
    pub(crate) fn set_waiting(&mut self, panes: Vec<FeedbackKey>) {
        self.waiting = panes;
    }

    pub(crate) fn keep(&mut self, batch: Batch, pending: Option<Arc<()>>) {
        let batch = Batch {
            text: fit(batch.text, TAKE_BUDGET),
            ..batch
        };
        if self.kept.len() >= MAX_KEPT {
            self.kept.pop_front();
        }
        self.added += 1;
        self.kept.push_back((batch, pending, self.added));
    }

    /// Keeps again, first in line, a batch that was taken but never reached
    /// its agent, so it still goes before every newer one. It is no new
    /// batch, so it raises no new event.
    pub(crate) fn keep_again(&mut self, batch: Batch) {
        let batch = Batch {
            text: fit(batch.text, TAKE_BUDGET),
            ..batch
        };
        if self.kept.len() >= MAX_KEPT {
            self.kept.pop_back();
        }
        self.kept.push_front((batch, None, 0));
    }

    /// Whether the batch `pending` follows is still kept.
    pub(crate) fn holds(&self, pending: &Weak<()>) -> bool {
        self.kept
            .iter()
            .any(|(_, kept, _)| crate::agent_notes::carries(kept.as_ref(), pending))
    }

    /// Drops the batch `pending` follows, before its agent fetched it.
    pub(crate) fn withdraw(&mut self, pending: &Weak<()>) -> bool {
        let kept = self.kept.len();
        self.kept
            .retain(|(_, held, _)| !crate::agent_notes::carries(held.as_ref(), pending));
        self.kept.len() < kept
    }

    /// What is kept for this daemon and pane, oldest first, joined into one
    /// text of at most [`TAKE_BUDGET`] (always at least one batch); the rest
    /// stays for the next take.
    pub(crate) fn take(&mut self, target: &FeedbackKey) -> Option<String> {
        let mut used = 0;
        let mut taken = Vec::new();
        let mut rest = VecDeque::with_capacity(self.kept.len());
        // Oldest first and in order: once one batch does not fit, every
        // later one for this pane waits too, so a newer note never arrives
        // before an older one.
        let mut full = false;
        for entry in self.kept.drain(..) {
            if !full && &entry.0.target == target {
                let size = json_size(&entry.0.text) + 1;
                if taken.is_empty() || used + size <= TAKE_BUDGET {
                    used += size;
                    taken.push(entry);
                    continue;
                }
                full = true;
            }
            rest.push_back(entry);
        }
        self.kept = rest;
        (!taken.is_empty()).then(|| {
            taken
                .into_iter()
                .map(|(batch, ..)| batch.text)
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    /// Whom the kept batches are for, oldest first, each with its number:
    /// numbers only grow, so a reader tells new batches from old ones even
    /// once the oldest are dropped.
    pub(crate) fn kept(&self) -> impl Iterator<Item = (u64, &FeedbackKey)> {
        self.kept
            .iter()
            .map(|(batch, _, added)| (*added, &batch.target))
    }

    pub(crate) fn has(&self, target: &FeedbackKey) -> bool {
        self.kept.iter().any(|(batch, ..)| &batch.target == target)
    }
}

#[cfg(test)]
mod tests;
