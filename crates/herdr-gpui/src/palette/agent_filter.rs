//! Go To's agent status filter, as Herdr's navigator offers with b/w/i/d/a.
//!
//! The status always comes from the daemon snapshot an entry was built from;
//! a terminal without an agent has none and only passes [`AgentFilter::All`].

use super::{Entry, Filter, Selection, search};
use crate::{HerdrWindow, i18n::fl};
use gpui::{Context, Keystroke, Modifiers};
use herdr_client::protocol::AgentStatus;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum AgentFilter {
    #[default]
    All,
    Blocked,
    Working,
    Idle,
    Done,
}

impl AgentFilter {
    pub(super) const ALL: [Self; 5] = [
        Self::All,
        Self::Blocked,
        Self::Working,
        Self::Idle,
        Self::Done,
    ];

    /// The chip's name in test selectors, the same in every language.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::All => "all-agents",
            Self::Blocked => "blocked",
            Self::Working => "working",
            Self::Idle => "idle",
            Self::Done => "done",
        }
    }

    pub(super) fn label(self) -> String {
        match self {
            Self::All => fl!("palette-go-to-filter-all"),
            Self::Blocked => fl!("palette-go-to-filter-blocked"),
            Self::Working => fl!("palette-go-to-filter-working"),
            Self::Idle => fl!("palette-go-to-filter-idle"),
            Self::Done => fl!("palette-go-to-filter-done"),
        }
    }

    /// Herdr's navigator letter, typed with Alt so the search keeps every
    /// plain letter.
    fn key(self) -> &'static str {
        match self {
            Self::All => "a",
            Self::Blocked => "b",
            Self::Working => "w",
            Self::Idle => "i",
            Self::Done => "d",
        }
    }

    /// The filter Alt and exactly one navigator letter choose. Any other
    /// modifier leaves the keystroke to the search field and the keymap.
    pub(super) fn from_keystroke(keystroke: &Keystroke) -> Option<Self> {
        if keystroke.modifiers != Modifiers::alt() {
            return None;
        }
        Self::ALL
            .into_iter()
            .find(|filter| filter.key() == keystroke.key)
    }

    /// Whether a row whose agent reports `status` stays listed; `None` is a
    /// row without an agent.
    pub(super) fn accepts(self, status: Option<AgentStatus>) -> bool {
        let wanted = match self {
            Self::All => return true,
            Self::Blocked => AgentStatus::Blocked,
            Self::Working => AgentStatus::Working,
            Self::Idle => AgentStatus::Idle,
            Self::Done => AgentStatus::Done,
        };
        status == Some(wanted)
    }

    /// Ranks `candidates` as [`search::rank`] does. While a status is chosen,
    /// the candidates are only matching agents, so each one's workspace and
    /// tab are added back as its headings, whether or not the query matches
    /// them, and a workspace ranks with the best of its agents.
    pub(super) fn rank(
        self,
        entries: &[Entry],
        candidates: Vec<usize>,
        query: &search::Query,
        matcher: &mut nucleo_matcher::Matcher,
    ) -> Vec<search::Hit> {
        let hits = search::rank(entries, candidates, query, matcher);
        if self == Self::All {
            return hits;
        }
        headed(entries, hits)
    }
}

/// `hits` grouped under their workspace, workspaces in the order of their
/// best hit and rows within one in entry order, so every agent stays nested
/// under its headings.
fn headed(entries: &[Entry], hits: Vec<search::Hit>) -> Vec<search::Hit> {
    let ancestors = |index: usize| {
        std::iter::successors(entries[index].parent, |parent| entries[*parent].parent)
    };
    let root = |index: usize| ancestors(index).last().unwrap_or(index);
    let mut groups = HashMap::new();
    let mut rows = BTreeMap::new();
    for hit in hits {
        let next = groups.len();
        groups.entry(root(hit.index)).or_insert(next);
        for heading in ancestors(hit.index) {
            rows.entry(heading)
                .or_insert_with(search::Highlights::default);
        }
        rows.insert(hit.index, hit.highlights);
    }
    let mut rows: Vec<_> = rows
        .into_iter()
        .map(|(index, highlights)| {
            let group = groups.get(&root(index)).copied().unwrap_or(usize::MAX);
            (group, index, highlights)
        })
        .collect();
    rows.sort_by_key(|(group, index, _)| (*group, *index));
    rows.into_iter()
        .map(|(_, index, highlights)| search::Hit {
            index,
            depth: ancestors(index).count(),
            highlights,
        })
        .collect()
}

impl HerdrWindow {
    /// Chooses a status and shows Go To, the only filter that applies one.
    pub(super) fn set_palette_agent_filter(
        &mut self,
        agent_filter: AgentFilter,
        cx: &mut Context<Self>,
    ) {
        if let Some(palette) = &mut self.menu.palette {
            palette.filter = Filter::Navigation;
            palette.agent_filter = agent_filter;
            palette.restore = None;
        }
        self.rank_palette(Selection::First, cx);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
