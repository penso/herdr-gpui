//! The order the sidebar lists workspaces and agents in, which the previous,
//! next, and numbered shortcuts step through, as Herdr's TUI steps through
//! its own sidebar: every listed host's rows, top to bottom.

use super::{
    agents::{groups::Groups, panel_agents},
    visible_workspace_entries,
};
use crate::{SheprdWindow, controls::Command, state::LiveState};
use sheprd_client::protocol::{ClientShellAgent, ClientShellSnapshot};

/// A listed row: its host, by endpoint index, and the workspace or agent
/// pane it shows.
pub(crate) type Row<'a> = (usize, &'a str);

impl SheprdWindow {
    /// Each host the sidebar lists, with the live state it draws from.
    fn listed_lives(&self) -> impl Iterator<Item = (usize, &LiveState)> {
        self.displayed_devices()
            .into_iter()
            .filter(|(_, endpoint)| endpoint.enabled && self.device_visible(&endpoint.id))
            .map(|(index, endpoint)| {
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                (index, live)
            })
    }

    /// Each host the sidebar lists, with the snapshot it draws from.
    fn listed_hosts(&self) -> impl Iterator<Item = (usize, &ClientShellSnapshot)> {
        self.listed_lives()
            .filter_map(|(index, live)| live.snapshot.as_deref().map(|snapshot| (index, snapshot)))
    }

    /// The workspace rows the sidebar shows: a folded group or host hides
    /// its rows here too.
    pub(crate) fn sidebar_workspaces(&self) -> Vec<Row<'_>> {
        let multi = self.endpoints.len() > 1;
        self.listed_hosts()
            .filter(|(index, _)| {
                let endpoint = &self.endpoints[*index];
                !(multi && (endpoint.collapsed || self.device_tag_folded(&endpoint.id)))
            })
            .flat_map(|(index, snapshot)| {
                let collapsed = if index == 0 {
                    &self.collapsed_repos
                } else {
                    &self.endpoints[index].collapsed_repos
                };
                visible_workspace_entries(&snapshot.workspaces, collapsed)
                    .into_iter()
                    .map(move |(row, _, _)| (index, snapshot.workspaces[row].workspace_id.as_str()))
            })
            .collect()
    }

    /// The agents the panel paints, by host index, in its order. Painting
    /// and stepping both read it, so a shortcut lands on the row below. A
    /// picked coordinator group narrows it for both.
    pub(super) fn panel_agents(&self) -> Vec<(usize, &ClientShellAgent)> {
        let rows = panel_agents(
            self.listed_lives(),
            self.agent_sort,
            &self.config.sidebar_style.status_priority,
        );
        let (Some(groups), Some(group)) = (self.agent_groups(), self.agent_group_filter()) else {
            return rows;
        };
        let hosts: Vec<_> = self.listed_hosts().collect();
        groups.narrow(
            group,
            rows.into_iter().filter_map(|(index, agent)| {
                let (_, snapshot) = hosts.iter().find(|(host, _)| *host == index)?;
                Some((index, agent, *snapshot))
            }),
        )
    }

    /// The suffix that names coordinators, while the agents panel is there to
    /// show groups: the other placements have no header to say a filter is on.
    fn panel_coordinator_suffix(&self) -> Option<&str> {
        let panel = self.config.show_agents
            && self.config.agents_placement == crate::config::AgentsPlacement::Panel
            && self.config.layout.mode.lists_workspaces();
        panel
            .then_some(self.config.sidebar_style.coordinator_suffix.as_deref())
            .flatten()
    }

    /// The coordinator groups of every listed host, or None while the agents
    /// panel offers none.
    pub(crate) fn agent_groups(&self) -> Option<Groups<'_>> {
        let suffix = self.panel_coordinator_suffix()?;
        let sessions = self
            .listed_hosts()
            .flat_map(|(_, snapshot)| snapshot.agents.iter().map(move |agent| (agent, snapshot)));
        Some(Groups::new(suffix, sessions, self.agent_group.as_deref()))
    }

    /// The group the agents panel is narrowed to.
    pub(super) fn agent_group_filter(&self) -> Option<&str> {
        self.panel_coordinator_suffix()?;
        self.agent_group.as_deref()
    }

    /// The agent panel's rows, whether or not the panel is shown, as Herdr
    /// steps through its agents either way.
    pub(crate) fn sidebar_agents(&self) -> Vec<Row<'_>> {
        self.panel_agents()
            .into_iter()
            .map(|(index, agent)| (index, agent.pane_id.as_str()))
            .collect()
    }
}

/// Which row a sidebar step lands on. `current` is the focused row, if the
/// sidebar lists it; `selected` is the selected host. Previous and next wrap
/// around, and start from an end when nothing listed is focused. A numbered
/// workspace counts only the selected host's rows and a numbered agent every
/// host's, as Herdr numbers them.
pub(crate) fn step(
    rows: &[Row<'_>],
    current: Option<usize>,
    selected: usize,
    command: Command,
) -> Option<usize> {
    let len = rows.len();
    if len == 0 {
        return None;
    }
    match command {
        Command::PreviousWorkspace | Command::PreviousAgent => {
            Some(current.map_or(len - 1, |index| (index + len - 1) % len))
        }
        Command::NextWorkspace | Command::NextAgent => {
            Some(current.map_or(0, |index| (index + 1) % len))
        }
        Command::WorkspaceNumber(number) => rows
            .iter()
            .enumerate()
            .filter(|(_, (host, _))| *host == selected)
            .nth(usize::from(number).checked_sub(1)?)
            .map(|(index, _)| index),
        Command::AgentNumber(number) => {
            let index = usize::from(number).checked_sub(1)?;
            (index < len).then_some(index)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_wrap_and_start_from_an_end() {
        let rows = [(0, "a"), (0, "b"), (1, "c")];
        for (command, current, expected) in [
            (Command::NextWorkspace, Some(0), Some(1)),
            (Command::NextWorkspace, Some(2), Some(0)),
            (Command::NextWorkspace, None, Some(0)),
            (Command::PreviousWorkspace, Some(0), Some(2)),
            (Command::PreviousWorkspace, Some(2), Some(1)),
            (Command::PreviousWorkspace, None, Some(2)),
            (Command::NextAgent, Some(1), Some(2)),
            (Command::PreviousAgent, None, Some(2)),
        ] {
            assert_eq!(step(&rows, current, 0, command), expected, "{command:?}");
        }
        for command in [Command::NextWorkspace, Command::PreviousAgent] {
            assert_eq!(step(&[], None, 0, command), None);
        }
        assert_eq!(step(&rows, Some(0), 0, Command::Tab), None);
    }

    #[test]
    fn numbers_count_from_one() {
        let rows = [(0, "a"), (1, "b"), (0, "c"), (1, "d")];
        // A numbered workspace counts the selected host's rows only.
        assert_eq!(step(&rows, None, 0, Command::WorkspaceNumber(2)), Some(2));
        assert_eq!(step(&rows, None, 1, Command::WorkspaceNumber(1)), Some(1));
        assert_eq!(step(&rows, None, 1, Command::WorkspaceNumber(3)), None);
        assert_eq!(step(&rows, None, 0, Command::WorkspaceNumber(0)), None);
        // A numbered agent counts every host's.
        assert_eq!(step(&rows, None, 0, Command::AgentNumber(4)), Some(3));
        assert_eq!(step(&rows, None, 0, Command::AgentNumber(5)), None);
        assert_eq!(step(&rows, None, 0, Command::AgentNumber(0)), None);
    }
}
