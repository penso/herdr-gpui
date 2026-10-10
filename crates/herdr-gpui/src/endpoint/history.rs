//! Back and Forward through the panes an endpoint has focused, the way a
//! browser walks its pages. The trail is client-local and observed from the
//! daemon's snapshots, so focus changed from anywhere (a key, the sidebar,
//! another client) becomes a step, and pane IDs never outlive their boot.

use herdr_client::protocol::ClientShellSnapshot;
use std::collections::VecDeque;

/// How many panes the trail remembers; older ones fall off the back.
const LIMIT: usize = 100;

/// Which way along the trail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Back,
    Forward,
}

#[derive(Debug, Default)]
pub(crate) struct History {
    boot: String,
    panes: VecDeque<String>,
    /// The entry the focused pane was reached as.
    cursor: usize,
    /// The entry a Back or Forward asked the daemon for. Until its focus
    /// arrives, the next step continues from there, and the arriving focus
    /// moves the cursor instead of cutting off the entries ahead.
    travel: Option<usize>,
}

impl History {
    /// Records the snapshot's focus. A snapshot without one, such as the
    /// empty state between connections, leaves the trail alone.
    ///
    /// `navigating` says whether a focus request is still in flight. A
    /// travel lives only that long: once the daemon has answered, failed, or
    /// the connection carrying it is gone, whatever focus arrived is the
    /// outcome, and later steps start from where the user really is.
    pub(crate) fn observe(&mut self, snapshot: Option<&ClientShellSnapshot>, navigating: bool) {
        if let Some(snapshot) = snapshot {
            self.record(snapshot);
        }
        if !navigating {
            self.travel = None;
        }
    }

    fn record(&mut self, snapshot: &ClientShellSnapshot) {
        if snapshot.boot_id != self.boot {
            *self = Self {
                boot: snapshot.boot_id.clone(),
                ..Self::default()
            };
        }
        let Some(focused) = snapshot.focused_pane_id.as_deref() else {
            return;
        };
        if self.entry(self.cursor) == Some(focused) {
            return;
        }
        if let Some(index) = self.travel.take()
            && self.entry(index) == Some(focused)
        {
            self.cursor = index;
            return;
        }
        // Somewhere new: like a browser, it replaces whatever was ahead.
        self.panes.truncate(self.cursor + 1);
        self.panes.push_back(focused.to_owned());
        if self.panes.len() > LIMIT {
            self.panes.pop_front();
        }
        self.cursor = self.panes.len() - 1;
    }

    /// Whether `step` has a pane to go to in `snapshot`.
    pub(crate) fn can(&self, step: Step, snapshot: &ClientShellSnapshot) -> bool {
        self.target(step, snapshot).is_some()
    }

    /// The entry `step` would go to and its pane, without going. The caller
    /// asks the daemon to focus the pane and calls [`Self::begin`] only once
    /// the request is queued, so a press that cannot be sent changes nothing.
    pub(crate) fn peek(&self, step: Step, snapshot: &ClientShellSnapshot) -> Option<(usize, &str)> {
        let index = self.target(step, snapshot)?;
        Some((index, self.entry(index)?))
    }

    /// Marks `index` as the travel in flight.
    pub(crate) fn begin(&mut self, index: usize) {
        if index < self.panes.len() {
            self.travel = Some(index);
        }
    }

    /// The nearest entry in `step`'s direction whose pane still exists and is
    /// not the one already focused. Closed panes stay in the trail and are
    /// stepped over, as Herdr's `last_pane` checks its pane.
    fn target(&self, step: Step, snapshot: &ClientShellSnapshot) -> Option<usize> {
        if snapshot.boot_id != self.boot {
            return None;
        }
        let from = self.travel.unwrap_or(self.cursor);
        let usable = |&index: &usize| {
            self.entry(index).is_some_and(|pane| {
                snapshot.focused_pane_id.as_deref() != Some(pane)
                    && snapshot.panes.iter().any(|p| p.pane_id == pane)
            })
        };
        match step {
            Step::Back => (0..from).rev().find(usable),
            Step::Forward => (from + 1..self.panes.len()).find(usable),
        }
    }

    fn entry(&self, index: usize) -> Option<&str> {
        self.panes.get(index).map(String::as_str)
    }
}

#[cfg(test)]
mod tests;
