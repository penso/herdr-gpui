//! The agents list: how it is sorted, and how each agent's place and status
//! are labelled. Status comes from the daemon's snapshot, never from guessing
//! at terminal output.

use super::first_text;
use crate::HerdrWindow;
use gpui_kit::component::{
    Disableable, IconName, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{Context, InteractiveElement, SharedString};
use herdr_client::protocol::{AgentStatus, ClientShellAgent, ClientShellSnapshot};

/// Toggles the agents' order. A daemon that names its own agent view owns the
/// order, so the button then only reports that view.
pub(super) fn agents_sort(window: &HerdrWindow, cx: &mut Context<HerdrWindow>) -> Button {
    let view = window
        .live
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.agent_view_label.clone());
    let fixed = view.is_some();
    let label: SharedString = view.unwrap_or_else(|| window.agent_sort.to_string()).into();
    Button::new("agents-sort")
        .debug_selector(|| "agents-sort".into())
        .ghost()
        .xsmall()
        .icon(IconName::SortDescending)
        .label(label)
        .disabled(fixed)
        .on_click(cx.listener(|this, _, _, cx| {
            cx.stop_propagation();
            this.agent_sort = this.agent_sort.toggled();
            this.agent_sort_modified = true;
            this.save_chrome();
            cx.notify();
        }))
}

/// Attention first, then the most recent change, as upstream orders it.
pub(super) fn status_priority(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Blocked => 4,
        AgentStatus::Done => 3,
        AgentStatus::Working => 2,
        AgentStatus::Idle => 1,
        AgentStatus::Unknown => 0,
    }
}

/// The agents of one endpoint in the order the panel paints them.
pub(super) fn sorted_agents(
    agents: &[ClientShellAgent],
    sort: crate::preferences::AgentSort,
) -> Vec<&ClientShellAgent> {
    let mut ordered: Vec<_> = agents.iter().collect();
    if sort == crate::preferences::AgentSort::Priority {
        ordered.sort_by_key(|agent| {
            (
                std::cmp::Reverse(status_priority(agent.agent_status)),
                std::cmp::Reverse(agent.state_change_seq),
            )
        });
    }
    ordered
}

/// What an agent is called wherever it is listed.
pub(crate) fn agent_name(agent: &ClientShellAgent) -> &str {
    first_text(
        [
            agent.display_agent.as_deref(),
            agent.name.as_deref(),
            agent.agent.as_deref(),
            agent.title.as_deref(),
        ],
        "agent",
    )
}

/// Upstream's default agent rows: host, workspace and tab on the first line,
/// the agent itself on the second. The tab only earns its place when the
/// workspace has more than one or the user named it, as upstream decides.
pub(super) fn agent_labels<'a>(
    agent: &'a ClientShellAgent,
    snapshot: &'a ClientShellSnapshot,
    host: Option<&'a str>,
) -> (Vec<(&'a str, bool)>, &'a str) {
    let name = agent_name(agent);
    // A pane whose workspace has gone leaves the agent to name the row.
    let Some(workspace) = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == agent.workspace_id)
        .map(|workspace| workspace.label.as_str())
    else {
        return (vec![(name, true)], "");
    };
    let tabs = snapshot
        .tabs
        .iter()
        .filter(|tab| tab.workspace_id == agent.workspace_id)
        .count();
    let tab = snapshot
        .tabs
        .iter()
        .find(|tab| tab.tab_id == agent.tab_id)
        .filter(|tab| tabs > 1 || tab.custom_label)
        .map(|tab| tab.label.as_str());
    // Only the workspace carries the row's weight: upstream paints the host and
    // tab around it in its secondary color.
    let segments = [(host, false), (Some(workspace), true), (tab, false)]
        .into_iter()
        .filter_map(|(text, primary)| Some((text?, primary)))
        .filter(|(text, _)| !text.is_empty())
        .collect();
    (segments, name)
}

/// An agent row's one line: the agent, then where it runs.
pub(super) fn agent_row_label(segments: &[(&str, bool)], name: &str) -> String {
    let place = segments
        .iter()
        .map(|(text, _)| *text)
        .collect::<Vec<_>>()
        .join(" \u{b7} ");
    if name.is_empty() {
        place
    } else {
        format!("{name} \u{b7} {place}")
    }
}
