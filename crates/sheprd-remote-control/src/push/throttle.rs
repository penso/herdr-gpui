//! One device's push budget: at most one push per chat every five seconds,
//! keeping the newest held back, and at most thirty an hour. Questions and
//! clears skip the per-chat wait, since a late question costs the user time,
//! but count toward the hourly cap like everything else.
//!
//! Times are explicit milliseconds, so tests need no clock. Memory is
//! bounded: a chat is remembered only for five seconds after a push, so no
//! more chats than the hourly cap are ever kept.

use super::{Notification, triggers::Kind};
use std::collections::{HashMap, VecDeque};

/// The shortest gap between two pushes for one chat.
pub(super) const CHAT_INTERVAL_MS: u64 = 5_000;
/// The window and size of the per-device cap.
pub(super) const HOUR_MS: u64 = 60 * 60 * 1000;
pub(super) const MAX_PER_HOUR: usize = 30;

#[derive(Debug, Default)]
pub(crate) struct Throttle {
    /// Chats pushed to within the last interval, by notification tag.
    chats: HashMap<String, Recent>,
    /// When each push of the last hour went out, oldest first.
    sent: VecDeque<u64>,
}

#[derive(Debug)]
struct Recent {
    last_ms: u64,
    /// The newest push held back, sent when the interval ends.
    pending: Option<Notification>,
}

impl Throttle {
    /// Offers a push at `now_ms`: returns it when it may go now, holds it
    /// back (replacing an older held one for the chat), or drops it once the
    /// hourly cap is reached.
    pub(crate) fn offer(
        &mut self,
        notification: Notification,
        now_ms: u64,
    ) -> Option<Notification> {
        self.forget(now_ms);
        let urgent = notification.clear || notification.kind == Kind::Question;
        if let Some(recent) = self.chats.get_mut(&notification.tag)
            && !urgent
            && now_ms.saturating_sub(recent.last_ms) < CHAT_INTERVAL_MS
        {
            recent.pending = Some(notification);
            return None;
        }
        self.admit(notification, now_ms)
    }

    /// The held pushes whose interval has ended by `now_ms`, to send now.
    pub(crate) fn due(&mut self, now_ms: u64) -> Vec<Notification> {
        self.forget(now_ms);
        let ready: Vec<Notification> = self
            .chats
            .values_mut()
            .filter(|recent| now_ms.saturating_sub(recent.last_ms) >= CHAT_INTERVAL_MS)
            .filter_map(|recent| recent.pending.take())
            .collect();
        let sent = ready
            .into_iter()
            .filter_map(|notification| self.admit(notification, now_ms))
            .collect();
        self.prune(now_ms);
        sent
    }

    /// Records `notification` as sent at `now_ms` unless the hourly cap is
    /// reached. A push replaces whatever its chat held back, being newer.
    fn admit(&mut self, notification: Notification, now_ms: u64) -> Option<Notification> {
        if self.sent.len() >= MAX_PER_HOUR {
            return None;
        }
        self.sent.push_back(now_ms);
        self.chats.insert(
            notification.tag.clone(),
            Recent {
                last_ms: now_ms,
                pending: None,
            },
        );
        Some(notification)
    }

    /// Drops pushes older than an hour from the cap and chats with nothing
    /// held whose interval has ended.
    fn forget(&mut self, now_ms: u64) {
        while self
            .sent
            .front()
            .is_some_and(|sent| now_ms.saturating_sub(*sent) >= HOUR_MS)
        {
            self.sent.pop_front();
        }
        self.prune(now_ms);
    }

    fn prune(&mut self, now_ms: u64) {
        self.chats.retain(|_, recent| {
            recent.pending.is_some() || now_ms.saturating_sub(recent.last_ms) < CHAT_INTERVAL_MS
        });
    }

    #[cfg(test)]
    pub(super) fn remembered(&self) -> usize {
        self.chats.len()
    }
}
