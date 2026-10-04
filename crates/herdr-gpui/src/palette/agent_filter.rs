//! Go To's agent status filter, as Herdr's navigator offers with b/w/i/d/a.
//!
//! The status always comes from the daemon snapshot an entry was built from;
//! a terminal without an agent has none and only passes [`AgentFilter::All`].

use gpui::{Keystroke, Modifiers};
use herdr_client::protocol::AgentStatus;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AgentFilter {
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

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::All => "All agents",
            Self::Blocked => "Blocked",
            Self::Working => "Working",
            Self::Idle => "Idle",
            Self::Done => "Done",
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn alt_and_a_navigator_letter_choose_a_filter() {
        let parse = |text| AgentFilter::from_keystroke(&Keystroke::parse(text).unwrap());
        for (text, expected) in [
            ("alt-a", AgentFilter::All),
            ("alt-b", AgentFilter::Blocked),
            ("alt-w", AgentFilter::Working),
            ("alt-i", AgentFilter::Idle),
            ("alt-d", AgentFilter::Done),
        ] {
            assert_eq!(parse(text), Some(expected), "{text}");
        }
        for text in [
            "b",
            "shift-b",
            "ctrl-b",
            "cmd-b",
            "alt-shift-b",
            "ctrl-alt-b",
            "alt-x",
            "alt-1",
        ] {
            assert_eq!(parse(text), None, "{text}");
        }
    }

    #[test]
    fn a_status_filter_keeps_only_agents_reporting_that_status() {
        let statuses = [
            None,
            Some(AgentStatus::Idle),
            Some(AgentStatus::Working),
            Some(AgentStatus::Blocked),
            Some(AgentStatus::Done),
            Some(AgentStatus::Unknown),
        ];
        for filter in AgentFilter::ALL {
            let kept: Vec<_> = statuses
                .into_iter()
                .filter(|status| filter.accepts(*status))
                .collect();
            let expected = match filter {
                AgentFilter::All => statuses.to_vec(),
                AgentFilter::Blocked => vec![Some(AgentStatus::Blocked)],
                AgentFilter::Working => vec![Some(AgentStatus::Working)],
                AgentFilter::Idle => vec![Some(AgentStatus::Idle)],
                AgentFilter::Done => vec![Some(AgentStatus::Done)],
            };
            assert_eq!(kept, expected, "{filter:?}");
        }
    }
}
