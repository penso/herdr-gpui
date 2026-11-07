//! Where a phone's question waits for Sheprd, which alone reads the
//! checkouts: the daemon cannot call it, so Sheprd takes what is queued here
//! and answers each by id. Bounded, and a question whose phone gave up is
//! forgotten, whichever side holds it.

use super::Error;
use crate::control::changes::{Answer, Ask, AskId, Failure, Wanted};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Condvar, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

/// Questions queued or being answered at once, from every phone.
const MAX_WAITING: usize = 16;
/// How long after Sheprd was last heard from it counts as open: longer than
/// its slowest poll, so a phone does not wait for a Sheprd that is gone.
const OPEN_FOR: Duration = Duration::from_secs(12);

#[derive(Default)]
pub(crate) struct Mailbox {
    arrived: Condvar,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    next: u64,
    queued: VecDeque<Ask>,
    /// Sheprd was last heard from then.
    seen: Option<Instant>,
    waiting: HashMap<AskId, oneshot::Sender<Answer>>,
}

/// A question on its way. Dropped, it is withdrawn.
pub(crate) struct Pending<'a> {
    answer: oneshot::Receiver<Answer>,
    id: AskId,
    mailbox: &'a Mailbox,
}

impl Pending<'_> {
    /// Sheprd's answer; `None` if the mailbox dropped the question.
    pub(crate) async fn answer(&mut self) -> Option<Answer> {
        (&mut self.answer).await.ok()
    }
}

impl Drop for Pending<'_> {
    fn drop(&mut self) {
        let mut inner = self.mailbox.inner();
        inner.waiting.remove(&self.id);
        inner.queued.retain(|ask| ask.id != self.id);
    }
}

impl Mailbox {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Sheprd is open now.
    pub(crate) fn seen(&self) {
        self.seen_at(Instant::now());
    }

    fn seen_at(&self, now: Instant) {
        self.inner().seen = Some(now);
    }

    /// Queues a question about a workspace of the session `boot_id`.
    pub(crate) fn ask(
        &self,
        boot_id: String,
        workspace_id: String,
        wanted: Wanted,
        now: Instant,
    ) -> Result<Pending<'_>, Error> {
        let mut inner = self.inner();
        if inner
            .seen
            .is_none_or(|seen| now.saturating_duration_since(seen) >= OPEN_FOR)
        {
            return Err(Error::NoDesktop);
        }
        if inner.waiting.len() >= MAX_WAITING {
            return Err(Error::Busy);
        }
        let id = AskId(inner.next);
        inner.next += 1;
        let (sender, answer) = oneshot::channel();
        inner.waiting.insert(id, sender);
        inner.queued.push_back(Ask {
            id,
            boot_id,
            workspace_id,
            wanted,
        });
        self.arrived.notify_all();
        Ok(Pending {
            answer,
            id,
            mailbox: self,
        })
    }

    /// Takes every queued question, waiting up to `wait` for one. Blocking:
    /// runs on the control connection's own thread.
    pub(crate) fn take(&self, wait: Duration) -> Vec<Ask> {
        self.seen();
        let (mut inner, _) = self
            .arrived
            .wait_timeout_while(self.inner(), wait, |inner| inner.queued.is_empty())
            .unwrap_or_else(PoisonError::into_inner);
        inner.seen = Some(Instant::now());
        inner.queued.drain(..).collect()
    }

    /// Hands Sheprd's answer to the phone waiting for it, if one still is.
    /// An answer past the bounds never reaches it.
    pub(crate) fn answer(&self, id: AskId, answer: Answer) {
        let answer = if answer.valid() {
            answer
        } else {
            tracing::warn!("Sheprd's answer about a workspace was out of bounds");
            Answer::Failed(Failure::Unreadable)
        };
        if let Some(waiting) = self.inner().waiting.remove(&id) {
            let _ = waiting.send(answer);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
