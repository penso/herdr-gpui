//! The search field at the top of the spaces list. While it holds a query,
//! the list gives way to flat results grouped by what matched: device names,
//! worktrees, then branches. Matching is a case-insensitive substring over
//! the snapshots the sidebar already holds, so typing never asks a daemon.

use super::{workspace_label, workspaces::workspace_entries};
use crate::{
    HerdrWindow, NavigationTarget,
    search_input::{Changed, SearchInput},
};
use gpui::{AppContext as _, Context, Entity, KeyDownEvent, ScrollHandle, Subscription, Window};
use herdr_client::protocol::{AgentStatus, ClientShellWorkspace};
use std::ops::Range;

mod results;

/// Results kept per query, and per section while scanning: enough for any
/// real sidebar, bounded for a huge one.
const RESULT_LIMIT: usize = 200;

pub(crate) struct SidebarSearch {
    pub(super) input: Entity<SearchInput>,
    query: String,
    /// The highlighted result, which Enter opens.
    selected: usize,
    scroll: ScrollHandle,
    _changed: Subscription,
}

impl SidebarSearch {
    pub(crate) fn new(cx: &mut Context<HerdrWindow>) -> Self {
        let input = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder("Devices, worktrees, branches", cx);
            input
        });
        let changed = cx.subscribe(&input, |this, input, _: &Changed, cx| {
            let search = &mut this.sidebar_search;
            search.query = input.read(cx).text().to_owned();
            search.selected = 0;
            // The first child is the section heading; the second is the first hit.
            search.scroll.scroll_to_item(1);
            cx.notify();
        });
        Self {
            input,
            query: String::new(),
            selected: 0,
            scroll: ScrollHandle::new(),
            _changed: changed,
        }
    }

    /// The query results are shown for: none while the field is blank.
    pub(super) fn query(&self) -> Option<&str> {
        Some(self.query.trim()).filter(|query| !query.is_empty())
    }
}

/// What a result opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Device { endpoint: String },
    Workspace { endpoint: String, workspace: String },
}

/// Which section a result is listed under, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Kind {
    Device,
    Worktree,
    Branch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Hit {
    pub kind: Kind,
    pub target: Target,
    /// The matched text, with `range` the matching bytes in it.
    pub text: String,
    pub range: Range<usize>,
    /// A quieter line placing the result: its host, branch, or workspace.
    pub context: String,
    /// A workspace's agent status; devices have none.
    pub status: Option<AgentStatus>,
}

/// A device as search sees it.
pub(super) struct Device<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub workspaces: &'a [ClientShellWorkspace],
}

/// Every result for `query` over `devices`, devices first, then worktrees,
/// then branches. Hosts are named in context only when there is more than one.
pub(super) fn search(query: &str, devices: &[Device<'_>]) -> Vec<Hit> {
    let multi = devices.len() > 1;
    let place = |device: &Device<'_>, detail: &str| match (multi, detail.is_empty()) {
        (true, false) => format!("{} · {detail}", device.label),
        (true, true) => device.label.to_owned(),
        (false, _) => detail.to_owned(),
    };
    // A section stops matching once full, so a broad query over a huge
    // snapshot builds no more than it can show.
    let (mut found, mut worktrees, mut branches) = (Vec::new(), Vec::new(), Vec::new());
    let room = |section: &Vec<Hit>| section.len() < RESULT_LIMIT;
    for device in devices {
        if room(&found)
            && let Some(range) = find(device.label, query)
        {
            let count = device.workspaces.len();
            found.push(Hit {
                kind: Kind::Device,
                target: Target::Device {
                    endpoint: device.id.to_owned(),
                },
                text: device.label.to_owned(),
                range,
                context: format!("{count} space{}", if count == 1 { "" } else { "s" }),
                status: None,
            });
        }
        // Use sidebar grouping and labels, including children of collapsed groups.
        for (index, child) in workspace_entries(device.workspaces) {
            let workspace = &device.workspaces[index];
            let label = workspace_label(workspace, child);
            let branch = workspace.branch.as_deref().unwrap_or_default();
            let target = || Target::Workspace {
                endpoint: device.id.to_owned(),
                workspace: workspace.workspace_id.clone(),
            };
            if room(&worktrees)
                && let Some(range) = find(label, query)
            {
                worktrees.push(Hit {
                    kind: Kind::Worktree,
                    target: target(),
                    text: label.to_owned(),
                    range,
                    context: place(device, branch),
                    status: Some(workspace.agent_status),
                });
            }
            if room(&branches)
                && let Some(range) = find(branch, query)
            {
                branches.push(Hit {
                    kind: Kind::Branch,
                    target: target(),
                    text: branch.to_owned(),
                    range,
                    context: place(device, label),
                    status: Some(workspace.agent_status),
                });
            }
        }
    }
    found.extend(worktrees);
    found.extend(branches);
    found.truncate(RESULT_LIMIT);
    found
}

/// The bytes of `text` matching `query`, ignoring case: the first place where
/// the lowercase forms agree, measured in `text`'s own bytes.
pub(super) fn find(text: &str, query: &str) -> Option<Range<usize>> {
    let needle: Vec<char> = query.trim().chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return None;
    }
    text.char_indices()
        .find_map(|(start, _)| prefix_len(&text[start..], &needle).map(|len| start..start + len))
}

/// The length in bytes of the start of `text` whose lowercase form begins
/// with `needle`. A character whose lowercase form is longer, such as `İ`
/// (`i` and a combining dot), matches whole once the needle ends inside it.
fn prefix_len(text: &str, needle: &[char]) -> Option<usize> {
    let mut pending = needle;
    for (offset, ch) in text.char_indices() {
        for lower in ch.to_lowercase() {
            let Some((first, rest)) = pending.split_first() else {
                break;
            };
            if *first != lower {
                return None;
            }
            pending = rest;
        }
        if pending.is_empty() {
            return Some(offset + ch.len_utf8());
        }
    }
    None
}

impl HerdrWindow {
    /// The results for the current query over every device the sidebar shows.
    pub(super) fn sidebar_search_hits(&self) -> Vec<Hit> {
        let Some(query) = self.sidebar_search.query() else {
            return Vec::new();
        };
        let devices: Vec<_> = self
            .endpoints
            .iter()
            .enumerate()
            .filter(|(_, endpoint)| self.device_visible(&endpoint.id))
            .map(|(index, endpoint)| {
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                Device {
                    id: &endpoint.id,
                    label: &endpoint.label,
                    workspaces: live
                        .snapshot
                        .as_deref()
                        .map_or(&[], |snapshot| &snapshot.workspaces),
                }
            })
            .collect();
        search(query, &devices)
    }

    /// Opens a result, clears the search, and gives the keyboard back to the
    /// terminal.
    pub(super) fn open_search_hit(
        &mut self,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            Target::Device { endpoint } => {
                self.select_endpoint(endpoint, cx);
            }
            Target::Workspace {
                endpoint,
                workspace,
            } => {
                self.navigate_endpoint(endpoint, NavigationTarget::Workspace(workspace), cx);
            }
        }
        self.clear_sidebar_search(window, cx);
    }

    fn clear_sidebar_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_search
            .input
            .update(cx, |input, cx| input.clear(cx));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// The keys the field leaves alone: arrows move through the results,
    /// Enter opens the highlighted one, and Escape clears the search.
    pub(super) fn sidebar_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar_search.input.read(cx).is_composing() {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.clear_sidebar_search(window, cx);
            cx.stop_propagation();
            return;
        }
        let hits = self.sidebar_search_hits();
        let selected = self
            .sidebar_search
            .selected
            .min(hits.len().saturating_sub(1));
        match key {
            "down" | "up" => {
                let selected = if key == "down" {
                    (selected + 1).min(hits.len().saturating_sub(1))
                } else {
                    selected.saturating_sub(1)
                };
                self.sidebar_search.selected = selected;
                if !hits.is_empty() {
                    // Headings are scroll children too, one before each section.
                    let headings = 1 + hits[..=selected]
                        .windows(2)
                        .filter(|pair| pair[0].kind != pair[1].kind)
                        .count();
                    self.sidebar_search
                        .scroll
                        .scroll_to_item(selected + headings);
                }
            }
            "enter" => {
                if let Some(hit) = hits.get(selected) {
                    self.open_search_hit(&hit.target, window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
