//! Which workspaces the tree shows and how each one is named. A repository and
//! its linked worktrees form one group, folded or unfolded locally: collapsing
//! changes this client's view only, never the daemon's state.

use super::row::{PrTag, first_text};
use crate::config::Theme;
use herdr_client::protocol::ClientShellWorkspace;
use std::collections::{HashMap, HashSet};

pub(super) fn workspace_entries(workspaces: &[ClientShellWorkspace]) -> Vec<(usize, bool)> {
    let mut groups = HashMap::<&str, (Option<usize>, Vec<usize>)>::new();
    for (index, workspace) in workspaces.iter().enumerate() {
        if let Some(worktree) = &workspace.worktree {
            let (parent, members) = groups.entry(&worktree.key).or_default();
            if !worktree.is_linked_worktree && parent.is_none() {
                *parent = Some(index);
            }
            members.push(index);
        }
    }
    let mut emitted = HashSet::new();
    let mut entries = Vec::with_capacity(workspaces.len());
    for (index, workspace) in workspaces.iter().enumerate() {
        let group = workspace.worktree.as_ref().and_then(|tree| {
            let (parent, members) = groups.get(tree.key.as_str())?;
            Some((tree.key.as_str(), (*parent)?, members))
        });
        if let Some((key, parent, members)) = group {
            if emitted.insert(key) {
                entries.push((parent, false));
                entries.extend(members.iter().filter(|&&i| i != parent).map(|&i| (i, true)));
            }
        } else {
            entries.push((index, false));
        }
    }
    entries
}

pub(super) fn visible_workspace_entries(
    workspaces: &[ClientShellWorkspace],
    collapsed: &HashSet<String>,
) -> Vec<(usize, bool, Option<String>)> {
    let entries = workspace_entries(workspaces);
    entries
        .iter()
        .enumerate()
        .filter_map(|(position, &(index, child))| {
            let key = workspaces[index].worktree.as_ref().map(|tree| &tree.key);
            if child && key.is_some_and(|key| collapsed.contains(key)) {
                return None;
            }
            let group = (!child && entries.get(position + 1).is_some_and(|entry| entry.1))
                .then(|| key.cloned())
                .flatten();
            Some((index, child, group))
        })
        .collect()
}

/// Cached pull request and dirty mark for a worktree row, if the prefetch and
/// the Git probe already have them. Rendering only reads: a missing entry
/// simply shows nothing, never a stale or guessed state.
pub(super) fn workspace_badge(
    workspace: &ClientShellWorkspace,
    cache: &crate::pull_request::Cache,
    git: &crate::git::Git,
    theme: &Theme,
) -> (Option<PrTag>, bool) {
    let Some((key, branch)) = workspace
        .worktree
        .as_ref()
        .map(|tree| tree.key.as_str())
        .zip(workspace.branch.as_deref())
    else {
        return (None, false);
    };
    (
        cache.peek(key, branch).map(|pr| PrTag::new(pr, theme)),
        git.dirty(key, branch).unwrap_or(false),
    )
}

pub(crate) fn workspace_label(workspace: &ClientShellWorkspace, indented: bool) -> &str {
    let branch = (indented && !workspace.custom_label)
        .then_some(workspace.branch.as_deref())
        .flatten()
        .map(|branch| branch.strip_prefix("worktree/").unwrap_or(branch));
    first_text([branch, Some(&workspace.label)], "workspace")
}
