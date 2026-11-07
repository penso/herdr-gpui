//! The alerts of one host that passed the notification policy, newest last,
//! so one missed while the user was away, or held back by quiet hours or by
//! delivery being off, can still be found. In memory only: a restart starts
//! the list afresh, and a click is fenced by the daemon boot as a toast's is.

use super::Notice;
use sheprd_client::protocol::SemanticNotificationKind as Kind;
use std::{collections::VecDeque, time::SystemTime};

/// Alerts kept per host; the page shows this many across hosts.
pub(crate) const LIMIT: usize = 48;

#[derive(Clone)]
pub(crate) struct Alert {
    pub id: u64,
    pub at: SystemTime,
    pub notice: Notice,
}

#[derive(Default)]
pub(crate) struct History {
    alerts: VecDeque<Alert>,
    /// Counts every alert ever kept, which also numbers them.
    recorded: u64,
}

impl History {
    /// Keeps `notice` if it is one of the alerts the list is for: an agent
    /// that finished or needs the user. Custom notices are the sender's own.
    pub(super) fn record(&mut self, notice: &Notice, at: SystemTime) {
        if !matches!(notice.kind, Kind::NeedsAttention | Kind::Finished) {
            return;
        }
        if self.alerts.len() == LIMIT {
            self.alerts.pop_front();
        }
        self.alerts.push_back(Alert {
            id: self.recorded,
            at,
            notice: notice.clone(),
        });
        self.recorded += 1;
    }

    pub(crate) fn alerts(&self) -> impl Iterator<Item = &Alert> {
        self.alerts.iter()
    }

    pub(crate) fn get(&self, id: u64) -> Option<&Alert> {
        self.alerts.iter().find(|alert| alert.id == id)
    }

    /// Moves each time an alert is kept, so a page can tell it has news.
    pub(crate) fn revision(&self) -> u64 {
        self.recorded
    }
}
