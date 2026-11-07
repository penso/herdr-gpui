//! `[tab_labels]`: what a tab strip calls a tab with no name of its own.
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TabLabels {
    /// Show the agent's terminal title in place of the tab's number.
    pub agent_title: bool,
}
