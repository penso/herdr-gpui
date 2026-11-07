//! Coordinator groups: sessions that belong together by name alone. A session
//! named with the configured suffix (`demo-coord`) coordinates the group its
//! name leaves (`demo`), and sessions named `demo-…` are its workers, in any
//! workspace or on any host. Nothing stores which pane belongs where, so a
//! replaced coordinator takes over its group by being given the same name.

use super::safe_caption;
use crate::SheprdWindow;
use crate::i18n::fl;
use crate::sidebar::label_text;
use gpui::{prelude::*, *};
use sheprd_client::protocol::{ClientShellAgent, ClientShellSnapshot};

/// What a session does in its group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GroupRole {
    Coordinator,
    Worker,
}

/// A session's group, and its role there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Member<'a> {
    pub(crate) group: &'a str,
    pub(crate) role: GroupRole,
}

/// The names a session goes by: `herdr agent start NAME` or `agent rename`,
/// then its pane's Rename label.
fn session_names<'a>(
    agent: &'a ClientShellAgent,
    snapshot: &'a ClientShellSnapshot,
) -> impl Iterator<Item = &'a str> {
    let label = snapshot
        .panes
        .iter()
        .find(|pane| pane.pane_id == agent.pane_id)
        .and_then(|pane| pane.label.as_deref());
    agent
        .name
        .as_deref()
        .into_iter()
        .chain(label)
        .map(str::trim)
}

/// The group a coordinator's name leaves once the suffix is off it.
fn coordinated_group<'a>(name: &'a str, suffix: &str) -> Option<&'a str> {
    name.strip_suffix(suffix).filter(|group| !group.is_empty())
}

/// Whether `name` is `group` and then a separator, so `demolition-1` stays out
/// of `demo`.
fn names_worker_of(name: &str, group: &str) -> bool {
    name.strip_prefix(group)
        .and_then(|rest| rest.chars().next())
        .is_some_and(|separator| !separator.is_alphanumeric())
}

/// Whether the session's name marks it as a coordinator.
pub(in crate::sidebar) fn is_coordinator(
    agent: &ClientShellAgent,
    snapshot: &ClientShellSnapshot,
    suffix: &str,
) -> bool {
    session_names(agent, snapshot).any(|name| coordinated_group(name, suffix).is_some())
}

/// The groups the listed sessions form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Groups<'a> {
    suffix: &'a str,
    /// Sorted, so the header steps through them in a stable order.
    names: Vec<&'a str>,
}

impl<'a> Groups<'a> {
    /// Every group with a coordinator listed. `selected` counts as one
    /// without: while its coordinator is being replaced, its workers stay in
    /// the view and the header keeps naming it.
    pub(crate) fn new(
        suffix: &'a str,
        sessions: impl IntoIterator<Item = (&'a ClientShellAgent, &'a ClientShellSnapshot)>,
        selected: Option<&'a str>,
    ) -> Self {
        let mut names: Vec<_> = sessions
            .into_iter()
            .flat_map(|(agent, snapshot)| session_names(agent, snapshot))
            .filter_map(|name| coordinated_group(name, suffix))
            .chain(selected)
            .collect();
        names.sort_unstable();
        names.dedup();
        Self { suffix, names }
    }

    pub(crate) fn names(&self) -> &[&'a str] {
        &self.names
    }

    /// The session's place. The longest group wins, so `demo-api-12` works
    /// for `demo-api-coord` even with a `demo-coord` listed.
    fn member(
        &self,
        agent: &'a ClientShellAgent,
        snapshot: &'a ClientShellSnapshot,
    ) -> Option<Member<'a>> {
        let coordinated = session_names(agent, snapshot)
            .find_map(|name| coordinated_group(name, self.suffix))
            .map(|group| Member {
                group,
                role: GroupRole::Coordinator,
            });
        coordinated.or_else(|| {
            session_names(agent, snapshot)
                .flat_map(|name| {
                    self.names
                        .iter()
                        .copied()
                        .filter(move |group| names_worker_of(name, group))
                })
                .max_by_key(|group| group.len())
                .map(|group| Member {
                    group,
                    role: GroupRole::Worker,
                })
        })
    }

    /// The rows of `group`, each still in the order it came. Its coordinators
    /// lead, so the way back from a worker is always the top of the list.
    pub(crate) fn narrow(
        &self,
        group: &str,
        rows: impl IntoIterator<Item = (usize, &'a ClientShellAgent, &'a ClientShellSnapshot)>,
    ) -> Vec<(usize, &'a ClientShellAgent)> {
        let mut kept: Vec<_> = rows
            .into_iter()
            .filter_map(|(index, agent, snapshot)| {
                let member = self.member(agent, snapshot)?;
                (member.group == group).then_some((member.role, index, agent))
            })
            .collect();
        kept.sort_by_key(|(role, _, _)| *role != GroupRole::Coordinator);
        kept.into_iter()
            .map(|(_, index, agent)| (index, agent))
            .collect()
    }

    /// The group a click on the header moves to from `current`: each group
    /// in turn, then back to all of them.
    fn after(&self, current: Option<&str>) -> Option<&'a str> {
        let Some(current) = current else {
            return self.names.first().copied();
        };
        let at = self.names.iter().position(|name| *name == current)?;
        self.names.get(at + 1).copied()
    }
}

/// The header's group filter, for a sidebar that has groups to pick from: its
/// name is the current group, and a click moves to the next one.
pub(in crate::sidebar) fn agents_group(
    window: &SheprdWindow,
    cx: &mut Context<SheprdWindow>,
) -> Option<Stateful<Div>> {
    let groups = window.agent_groups()?;
    if groups.names().is_empty() {
        return None;
    }
    let theme = &window.theme;
    let label = window
        .agent_group
        .as_deref()
        .map_or_else(|| fl!("sidebar-group-all"), group_label);
    let next = groups
        .after(window.agent_group.as_deref())
        .map(str::to_owned);
    Some(
        window
            .chrome_stop(
                crate::window::chrome_focus::Region::Sidebar,
                div()
                    .id("agents-group")
                    .debug_selector(|| "agents-group".into())
                    .role(Role::Button)
                    .aria_label(fl!("sidebar-agents-group", group = label.as_str())),
            )
            .min_w_0()
            .truncate()
            .cursor_pointer()
            .text_color(rgb(if window.agent_group.is_some() {
                theme.foreground
            } else {
                theme.muted
            }))
            .hover(|style| style.text_color(rgb(theme.foreground)))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.agent_group = next.clone();
                cx.notify();
            }))
            .child(label_text(&label)),
    )
}

/// A group's name as the header shows it. Names come from the daemon and from
/// terminals, so they are untrusted: safe, bounded text, and a stand-in for a
/// name with nothing printable left, so the button can still be seen.
fn group_label(group: &str) -> String {
    safe_caption(Some(group)).unwrap_or_else(|| "?".to_owned()) // i18n: data
}

#[cfg(test)]
mod tests;
