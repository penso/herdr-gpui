//! What an agent notice is about, read from the snapshot the client already
//! holds. Herdr's own body names the workspace by its daemon label and number
//! (`10 · 1`), which says little once several agents run in numbered or
//! worktree workspaces.

use super::safe_text;
use sheprd_client::protocol::{ClientShellAgent, ClientShellSnapshot};

const PART_LIMIT: usize = 120;

/// The agent's task on the first line and its place, named as the sidebar
/// names it, on the second. `None` when the snapshot knows neither, so the
/// daemon's body stays.
pub(super) fn describe(snapshot: &ClientShellSnapshot, pane_id: &str) -> Option<String> {
    let pane = snapshot.panes.iter().find(|p| p.pane_id == pane_id)?;
    let agent = snapshot.agents.iter().find(|a| a.pane_id == pane_id);
    let task = agent.and_then(task);
    let place = place(snapshot, &pane.workspace_id, &pane.tab_id);
    match (task, place) {
        (Some(task), Some(place)) => Some(format!("{task}\n{place}")),
        (Some(text), None) | (None, Some(text)) => Some(text),
        (None, None) => None,
    }
}

/// The title the agent reported, else its terminal title, unless that only
/// repeats the agent's name.
fn task(agent: &ClientShellAgent) -> Option<String> {
    let task = [
        agent.title.as_deref(),
        agent.terminal_title_stripped.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(|title| safe_text(title, PART_LIMIT))
    .map(|title| title.trim().to_owned())
    .find(|title| !title.is_empty())?;
    let names = [
        agent.name.as_deref(),
        agent.display_agent.as_deref(),
        agent.agent.as_deref(),
    ];
    (!names
        .into_iter()
        .flatten()
        .any(|name| name.trim().eq_ignore_ascii_case(&task)))
    .then_some(task)
}

/// The workspace as its sidebar row shows it, plus the tab when the
/// workspace has several or the user named it.
fn place(snapshot: &ClientShellSnapshot, workspace_id: &str, tab_id: &str) -> Option<String> {
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|w| w.workspace_id == workspace_id)?;
    let linked = workspace
        .worktree
        .as_ref()
        .is_some_and(|worktree| worktree.is_linked_worktree);
    let mut place = safe_text(
        crate::sidebar::workspace_label(workspace, linked),
        PART_LIMIT,
    );
    let tabs = snapshot
        .tabs
        .iter()
        .filter(|tab| tab.workspace_id == workspace_id)
        .count();
    if let Some(tab) = snapshot
        .tabs
        .iter()
        .find(|tab| tab.tab_id == tab_id)
        .filter(|tab| tabs > 1 || tab.custom_label)
        .map(|tab| safe_text(&tab.label, PART_LIMIT))
        .filter(|label| !label.trim().is_empty())
    {
        place.push_str(" · ");
        place.push_str(&tab);
    }
    (!place.trim().is_empty()).then_some(place)
}
