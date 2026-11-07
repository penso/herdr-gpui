//! What the strip calls a tab: the daemon's label, or, with
//! `[tab_labels] agent_title` on, the terminal title of the agent in a tab
//! that has no name of its own. Only the strip's own drawing reads this; the
//! rename and close dialogs keep the daemon's label.
use crate::SheprdWindow;
use gpui::SharedString;
use sheprd_client::protocol::{ClientShellAgent, ClientShellSnapshot, ClientShellTab};

/// Grapheme clusters kept of a title before it is cut to a tab's width. It
/// only bounds the work on untrusted text, so it sits well past that width.
const TITLE_BOUND: usize = 64;

impl SheprdWindow {
    /// The label the strip, its drag ghost, and its stand-in show for `tab`.
    pub(in crate::window) fn strip_label(
        &self,
        snapshot: &ClientShellSnapshot,
        tab: &ClientShellTab,
    ) -> SharedString {
        self.config
            .tab_labels
            .agent_title
            .then(|| title(snapshot, tab))
            .flatten()
            .unwrap_or_else(|| sheprd_client::text::tab_label(snapshot, tab).to_owned())
            .into()
    }
}

/// The title of `tab`'s first agent, unless the tab was named or that agent
/// has none. A terminal title is untrusted, so it is made one safe line
/// before it is measured.
pub(super) fn title(snapshot: &ClientShellSnapshot, tab: &ClientShellTab) -> Option<String> {
    if tab.custom_label {
        return None;
    }
    let title = first_agent(snapshot, tab)?
        .terminal_title_stripped
        .as_deref()?;
    let line = crate::truncate::one_line(title, TITLE_BOUND);
    (!line.is_empty()).then(|| crate::truncate::tab_title(&line))
}

/// The first of `tab`'s agents in `agent_order`, the daemon's layout order.
/// A plugin's agent view narrows that order and is authoritative while the
/// daemon names it, so a tab it leaves out has no agent here. Without a view
/// an agent the order does not list still counts, in the snapshot's order.
fn first_agent<'a>(
    snapshot: &'a ClientShellSnapshot,
    tab: &ClientShellTab,
) -> Option<&'a ClientShellAgent> {
    // Runs on every draw: the tab's few agents are found once, so the order
    // is not searched against every agent of every workspace.
    let held: Vec<_> = snapshot
        .agents
        .iter()
        .filter(|agent| agent.tab_id == tab.tab_id)
        .collect();
    let ordered = snapshot
        .agent_order
        .iter()
        .find_map(|pane| held.iter().find(|agent| agent.pane_id == *pane).copied());
    if snapshot.agent_view_label.is_some() {
        return ordered;
    }
    ordered.or_else(|| held.first().copied())
}
