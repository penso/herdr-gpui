//! The names the sidebar's rows give screen readers. A row paints its parts
//! as plain text, which assistive technology never sees, so each row is one
//! list item whose name says what the row shows, most telling part first.

pub(super) use crate::accessibility::expandable;
use crate::{HerdrWindow, i18n::fl, window::chrome_focus::Region};
use gpui::{Context, Role, StatefulInteractiveElement};
use herdr_client::protocol::AgentStatus;

impl HerdrWindow {
    /// A sidebar button: its name, and its place among the sidebar's stops.
    pub(super) fn sidebar_button<E: StatefulInteractiveElement>(
        &self,
        button: E,
        label: impl Into<gpui::SharedString>,
    ) -> E {
        self.chrome_stop(Region::Sidebar, button.role(Role::Button).aria_label(label))
    }

    /// A workspace row as a list item named `name`. A repository's row folds
    /// its linked worktrees: `folds` is whether they are folded, and the
    /// endpoint and repository key that fold them.
    pub(super) fn workspace_item<E: StatefulInteractiveElement>(
        &self,
        row: E,
        name: String,
        selected: bool,
        folds: Option<(bool, (String, String))>,
        cx: &mut Context<Self>,
    ) -> E {
        let row = row
            .role(Role::ListItem)
            .aria_label(name)
            .aria_selected(selected);
        let row = self.chrome_stop(Region::Sidebar, row);
        let Some((collapsed, (endpoint, key))) = folds else {
            return row;
        };
        expandable(
            row,
            !collapsed,
            cx.entity().downgrade(),
            move |this, _, cx| {
                this.toggle_repository(&endpoint, &key, cx);
            },
        )
    }
}

/// A workspace row: its label, whether it is a linked worktree, then its
/// status and whether its notifications are muted. A row on another device
/// names that device too, since the list is flat. The worktree is named
/// whatever the sidebar draws, since its hue and glyph reach no screen reader.
pub(super) fn workspace_name(
    label: &str,
    linked: bool,
    status: AgentStatus,
    muted: bool,
    host: Option<&str>,
) -> String {
    let linked = linked.then(|| fl!("sidebar-spoken-linked-worktree"));
    let status = status_word(status);
    join([
        Some(label),
        linked.as_deref(),
        status.as_deref(),
        muted.then(|| fl!("sidebar-spoken-muted")).as_deref(),
        host,
    ])
}

/// An agent row: the agent, the name of its pane (what tells two agents of
/// one name apart), its state (the agent's own label for it, when it gave
/// one), and where it runs.
pub(super) fn agent_name(
    name: &str,
    pane: Option<&str>,
    state: &str,
    place: Option<(&str, Option<&str>)>,
    host: Option<&str>,
) -> String {
    let (workspace, tab) = place.unzip();
    join([
        Some(name),
        pane,
        Some(state),
        workspace,
        tab.flatten(),
        host,
    ])
}

/// A device's header: its name, whether it is connected, and, in the Devices
/// layout, how many of its agents work.
pub(super) fn device_name(label: &str, connected: bool, working: Option<&str>) -> String {
    let working = working.map(|count| fl!("sidebar-spoken-working", count = count));
    let connection = if connected {
        fl!("sidebar-spoken-connected")
    } else {
        fl!("sidebar-spoken-disconnected")
    };
    join([Some(label), Some(&connection), working.as_deref()])
}

/// `Unknown` says nothing a listener could use.
fn status_word(status: AgentStatus) -> Option<String> {
    (status != AgentStatus::Unknown).then(|| super::agents::status_text(status))
}

fn join<'a>(parts: impl IntoIterator<Item = Option<&'a str>>) -> String {
    parts
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(&fl!("sidebar-spoken-separator"))
}

#[cfg(test)]
mod tests;
