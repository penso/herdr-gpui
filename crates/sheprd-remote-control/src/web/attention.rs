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
    collections::{BTreeSet, HashMap},
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
    /// Finished panes a phone opened, with the finish that was seen. One
    /// that finished again since counts again, however quickly it did, so
    /// nothing depends on noticing the work in between.
    opened: HashMap<String, Finish>,
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

/// Tells one finish of an agent from the next: when it happened, if known.
type Finish = Option<u64>;

/// What a chat's transcript says, beyond the layout.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ChatFacts {
    pub(crate) finish: Finish,
    pub(crate) questions: usize,
}

/// An agent as this module sees it.
#[derive(Clone, Copy)]
struct Agent {
    finish: Finish,
    status: AgentStatus,
}

/// Every agent by pane, in chats and terminals alike.
fn agents(layout: &Layout, chat: impl Fn(&str) -> ChatFacts) -> HashMap<&str, Agent> {
    let chats = layout.chats().map(|row| {
        let finish = chat(&row.pane).finish;
        let status = row.status;
        (row.pane.as_str(), Agent { finish, status })
    });
    let terminals = layout
        .workspaces
        .iter()
        .flat_map(|workspace| workspace.terminals.iter())
        .filter_map(|terminal| terminal.agent.as_ref())
        .map(|agent| {
            let finish = agent.changed_ms;
            let status = agent.status;
            (agent.pane.as_str(), Agent { finish, status })
        });
    chats.chain(terminals).collect()
}

impl Opened {
    fn marks(&self) -> MutexGuard<'_, Marks> {
        self.marks.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records that a phone showed `pane`; only a finished agent stays
    /// recorded, since one still working has not finished what was seen.
    pub(crate) fn open(&self, layout: &Layout, pane: &str, chat: impl Fn(&str) -> ChatFacts) {
        let agents = agents(layout, chat);
        let Some(agent) = agents.get(pane).filter(|agent| is_done(agent)) else {
            return;
        };
        let seen = self.marks().opened.insert(pane.to_owned(), agent.finish);
        if seen != Some(agent.finish) {
            self.changed
                .send_modify(|moved| *moved = moved.wrapping_add(1));
        }
    }

    /// A view of `pane` opened.
    pub(super) fn begin(&self, layout: &Layout, pane: &str, chat: impl Fn(&str) -> ChatFacts) {
        *self.marks().watching.entry(pane.to_owned()).or_default() += 1;
        self.open(layout, pane, chat);
    }

    /// A view of `pane` closed. What finished in it was seen, even if
    /// nothing asked for the count while it was open.
    pub(super) fn end(&self, layout: &Layout, pane: &str, chat: impl Fn(&str) -> ChatFacts) {
        self.open(layout, pane, chat);
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

    /// The panes that want the user now; `chat` says what a chat's
    /// transcript holds. Also forgets what no longer applies.
    pub(crate) fn needing(
        &self,
        layout: &Layout,
        chat: impl Fn(&str) -> ChatFacts,
    ) -> BTreeSet<String> {
        let agents = agents(layout, &chat);
        let mut marks = self.marks();
        let Marks { opened, watching } = &mut *marks;
        opened.retain(|pane, seen| {
            agents
                .get(pane.as_str())
                .is_some_and(|agent| is_done(agent) && agent.finish == *seen)
        });
        for pane in watching.keys() {
            if let Some(agent) = agents.get(pane.as_str()).filter(|agent| is_done(agent)) {
                opened.insert(pane.clone(), agent.finish);
            }
        }
        let chats = layout.chats().map(|row| row.pane.as_str());
        let asking = chats.filter(|pane| chat(pane).questions > 0);
        let waiting = agents
            .iter()
            .filter(|(pane, agent)| match agent.status {
                AgentStatus::Blocked => true,
                AgentStatus::Done => !opened.contains_key(**pane),
                AgentStatus::Idle | AgentStatus::Unknown | AgentStatus::Working => false,
            })
            .map(|(pane, _)| *pane);
        asking.chain(waiting).map(str::to_owned).collect()
    }
}

fn is_done(agent: &Agent) -> bool {
    agent.status == AgentStatus::Done
}

/// A phone's view of one pane, for as long as this lives.
pub(crate) struct Watching {
    pane: String,
    state: Shared,
}

impl Watching {
    pub(crate) fn new(state: &Shared, pane: &str) -> Self {
        state
            .opened
            .begin(&state.hub.layout(), pane, |pane| state.chat_facts(pane));
        Self {
            pane: pane.to_owned(),
            state: state.clone(),
        }
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        let state = &self.state;
        state.opened.end(&state.hub.layout(), &self.pane, |pane| {
            state.chat_facts(pane)
        });
    }
}

impl State {
    /// What `pane`'s chat holds; nothing for a pane that is not a chat.
    fn chat_facts(&self, pane: &str) -> ChatFacts {
        self.hub.chat(pane).map_or_else(ChatFacts::default, |link| {
            let summary = link.summary();
            ChatFacts {
                finish: summary.last_turn_ended_ms,
                questions: summary.questions,
            }
        })
    }

    /// Records that a phone showed `pane`.
    pub(crate) fn mark_opened(&self, layout: &Layout, pane: &str) {
        self.opened.open(layout, pane, |pane| self.chat_facts(pane));
    }

    /// The panes that want the user now.
    pub(crate) fn attention(&self) -> BTreeSet<String> {
        self.opened
            .needing(&self.hub.layout(), |pane| self.chat_facts(pane))
    }
}
