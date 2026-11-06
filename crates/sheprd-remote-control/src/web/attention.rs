//! Which agents want the user: the number on the app's badge. An agent
//! counts while it needs an answer, and from finishing until someone looks.
//!
//! Herdr keeps `Done` until the pane is focused in one of its own clients,
//! which a phone is not, so this daemon remembers what a phone opened. One
//! list serves every phone: opening a chat on one clears it on all of them,
//! and looking at it on the computer clears it through Herdr. The computer's
//! own activity dots still come from Herdr's snapshot alone.
//!
//! Kept in memory: after a restart, finished agents count again until seen.

use super::{Shared, State};
use crate::hub::Layout;
use sheprd_client::protocol::AgentStatus;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::{Mutex, MutexGuard, PoisonError},
};
use tokio::sync::watch;

/// What phones have looked at.
#[derive(Debug)]
pub(crate) struct Opened {
    /// Moves when a pane stops counting, for views of the count to refresh.
    changed: watch::Sender<u64>,
    marks: Mutex<Marks>,
}

#[derive(Debug, Default)]
struct Marks {
    /// Finished panes a phone opened; a pane leaves when it is no longer
    /// `Done`, so its next finish counts again.
    opened: HashSet<String>,
    /// Views open on each pane now: what finishes in one was seen.
    watching: HashMap<String, usize>,
}

impl Default for Opened {
    fn default() -> Self {
        Self {
            changed: watch::channel(0).0,
            marks: Mutex::default(),
        }
    }
}

/// Every agent's pane and status, in chats and terminals alike.
fn statuses(layout: &Layout) -> HashMap<&str, AgentStatus> {
    let chats = layout.chats().map(|chat| (chat.pane.as_str(), chat.status));
    let terminals = layout
        .workspaces
        .iter()
        .flat_map(|workspace| workspace.terminals.iter())
        .filter_map(|terminal| terminal.agent.as_ref())
        .map(|agent| (agent.pane.as_str(), agent.status));
    chats.chain(terminals).collect()
}

impl Opened {
    fn marks(&self) -> MutexGuard<'_, Marks> {
        self.marks.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records that a phone showed `pane`; only a finished agent stays
    /// recorded, since one still working has not finished what was seen.
    pub(crate) fn open(&self, layout: &Layout, pane: &str) {
        if statuses(layout).get(pane) == Some(&AgentStatus::Done)
            && self.marks().opened.insert(pane.to_owned())
        {
            self.changed
                .send_modify(|moved| *moved = moved.wrapping_add(1));
        }
    }

    /// A view of `pane` opened.
    pub(super) fn begin(&self, layout: &Layout, pane: &str) {
        *self.marks().watching.entry(pane.to_owned()).or_default() += 1;
        self.open(layout, pane);
    }

    /// A view of `pane` closed. What finished in it was seen, even if
    /// nothing asked for the count while it was open.
    pub(super) fn end(&self, layout: &Layout, pane: &str) {
        self.open(layout, pane);
        let mut marks = self.marks();
        if let Some(views) = marks.watching.get_mut(pane) {
            *views -= 1;
            if *views == 0 {
                marks.watching.remove(pane);
            }
        }
    }

    /// Moves whenever a pane stops counting.
    pub(crate) fn changed(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    /// The panes that want the user now; `questions` says how many a chat
    /// has waiting. Also forgets what no longer applies.
    pub(crate) fn needing(
        &self,
        layout: &Layout,
        questions: impl Fn(&str) -> usize,
    ) -> BTreeSet<String> {
        let statuses = statuses(layout);
        let mut marks = self.marks();
        let Marks { opened, watching } = &mut *marks;
        opened.retain(|pane| statuses.get(pane.as_str()) == Some(&AgentStatus::Done));
        opened.extend(
            watching
                .keys()
                .filter(|pane| statuses.get(pane.as_str()) == Some(&AgentStatus::Done))
                .cloned(),
        );
        let chats = layout.chats().map(|chat| chat.pane.as_str());
        let asking = chats.filter(|pane| questions(pane) > 0);
        let waiting = statuses
            .iter()
            .filter(|(pane, status)| match status {
                AgentStatus::Blocked => true,
                AgentStatus::Done => !opened.contains(**pane),
                AgentStatus::Idle | AgentStatus::Unknown | AgentStatus::Working => false,
            })
            .map(|(pane, _)| *pane);
        asking.chain(waiting).map(str::to_owned).collect()
    }
}

/// A phone's view of one pane, for as long as this lives.
pub(crate) struct Watching {
    pane: String,
    state: Shared,
}

impl Watching {
    pub(crate) fn new(state: &Shared, pane: &str) -> Self {
        state.opened.begin(&state.hub.layout(), pane);
        Self {
            pane: pane.to_owned(),
            state: state.clone(),
        }
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        self.state.opened.end(&self.state.hub.layout(), &self.pane);
    }
}

impl State {
    /// The panes that want the user now.
    pub(crate) fn attention(&self) -> BTreeSet<String> {
        self.opened.needing(&self.hub.layout(), |pane| {
            self.hub
                .chat(pane)
                .map_or(0, |link| link.summary().questions)
        })
    }
}
