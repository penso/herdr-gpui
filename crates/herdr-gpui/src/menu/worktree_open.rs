//! Existing checkouts come only from the selected daemon, never local Git.

use super::{MenuState, Page, WorkspaceAction, endpoint_error, listener};
use crate::HerdrWindow;
use gpui_kit::{
    component::{
        ActiveTheme as _, h_flex,
        input::{Input, InputEvent, InputState},
        list::ListItem,
        v_flex,
    },
    prelude::*,
    *,
};
use herdr_client::Method;
use serde::Deserialize;

const MAX_ENTRIES: usize = 512;

#[derive(Deserialize)]
pub(super) struct Source {
    pub(super) repo_key: String,
    repo_name: String,
    source_workspace_id: Option<String>,
}

#[derive(serde::Deserialize)]
pub(super) struct Entry {
    pub(super) path: String,
    pub(super) branch: Option<String>,
    pub(super) label: String,
    is_bare: bool,
    is_prunable: bool,
    pub(super) is_detached: bool,
    pub(super) open_workspace_id: Option<String>,
}

impl Entry {
    /// Whether a lowercased search matches the checkout's path, label or branch.
    pub(super) fn matches(&self, query: &str) -> bool {
        [&self.path, &self.label]
            .into_iter()
            .map(String::as_str)
            .chain(self.branch.as_deref())
            .any(|text| text.to_lowercase().contains(query))
    }
}

pub(super) struct Picker {
    pub(super) search: Entity<InputState>,
    _subscription: Option<Subscription>,
    pub(super) pending: Option<String>,
    pub(super) entries: Vec<Entry>,
    pub(super) source: Option<Source>,
    pub(super) filtered: Vec<usize>,
    query: String,
    pub(super) selected: usize,
    pub(super) scroll: ScrollHandle,
}

impl Picker {
    pub(super) fn new(search: Entity<InputState>) -> Self {
        Self {
            search,
            _subscription: None,
            pending: None,
            entries: Vec::new(),
            source: None,
            filtered: Vec::new(),
            query: String::new(),
            selected: 0,
            scroll: ScrollHandle::new(),
        }
    }

    pub(super) fn filter(&mut self, query: &str) {
        self.query = query.trim().to_lowercase();
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.matches(&self.query))
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
        self.scroll.scroll_to_item(0);
    }

    /// Moves the highlight by `step` rows, wrapping, and keeps it in view.
    pub(super) fn step(&mut self, step: isize) {
        let rows = self.filtered.len();
        if rows == 0 || self.pending.is_some() {
            return;
        }
        self.selected = (self.selected as isize + step).rem_euclid(rows as isize) as usize;
        self.scroll.scroll_to_item(self.selected);
    }

    pub(super) fn entry(&self, row: usize) -> Option<&Entry> {
        self.entries.get(*self.filtered.get(row)?)
    }
}

pub(super) fn listing(response: &serde_json::Value) -> crate::Result<(Source, Vec<Entry>)> {
    let result = &response["result"];
    let rows = result["worktrees"]
        .as_array()
        .filter(|rows| result["type"] == "worktree_list" && rows.len() <= MAX_ENTRIES)
        .ok_or(crate::Error::WorktreeList)?;
    for field in ["repo_key", "repo_name", "source_workspace_id"] {
        if result["source"][field]
            .as_str()
            .is_some_and(|text| text.is_empty() || text.len() > 8192 || text.contains('\0'))
        {
            return Err(crate::Error::WorktreeList);
        }
    }
    let source =
        Source::deserialize(&result["source"]).map_err(crate::Error::WorktreeListDecode)?;
    let mut entries = Vec::with_capacity(rows.len());
    let mut paths = std::collections::HashSet::new();
    for row in rows {
        // Bound strings before deserializing/copying untrusted endpoint data.
        for field in ["path", "label", "branch", "open_workspace_id"] {
            if row[field]
                .as_str()
                .is_some_and(|text| text.len() > 8192 || text.contains('\0'))
            {
                return Err(crate::Error::WorktreeList);
            }
        }
        let entry = Entry::deserialize(row).map_err(crate::Error::WorktreeListDecode)?;
        if entry.path.is_empty() || !paths.insert(entry.path.clone()) {
            return Err(crate::Error::WorktreeList);
        }
        if !entry.is_bare && !entry.is_prunable {
            entries.push(entry);
        }
    }
    Ok((source, entries))
}

impl MenuState {
    pub(super) fn apply_worktree_list_response(
        &mut self,
        id: &str,
        result: crate::state::DialogResponse,
    ) {
        let Some(picker) = &mut self.worktree_open else {
            return;
        };
        if picker.pending.as_deref() != Some(id) {
            return;
        }
        picker.pending = None;
        self.error = match result {
            Err(error) => Some(error.to_string()),
            Ok(response) => {
                if let Some(error) = response.get("error") {
                    let (code, message) = endpoint_error(error);
                    Some(format!("{code}: {message}"))
                } else {
                    match listing(&response) {
                        Ok((source, entries)) => {
                            picker.source = Some(source);
                            picker.entries = entries;
                            picker.filter(&picker.query.clone());
                            None
                        }
                        Err(error) => Some(error.to_string()),
                    }
                }
            }
        };
    }
}

impl HerdrWindow {
    pub(super) fn worktree_open_current(&self) -> bool {
        if !self.menu_target_current() || !self.live.status.is_connected() {
            return false;
        }
        let Some((target, snapshot)) = self.menu.target.as_ref().zip(self.live.snapshot.as_ref())
        else {
            return false;
        };
        if snapshot.boot_id != target.boot_id {
            return false;
        }
        if target.validate_repository(snapshot).is_ok() {
            return true;
        }
        // Opening can establish the branch-only source's parent membership before
        // its response reaches the UI. Allow only the listed identity, while pending.
        if self.menu.page != Some(Page::Dialog(WorkspaceAction::OpenWorktree))
            || self.menu.creation.is_none()
            || target.worktree.is_some()
            || target.branch.is_none()
        {
            return false;
        }
        let Some(source) = self
            .menu
            .worktree_open
            .as_ref()
            .filter(|picker| picker.pending.is_none())
            .and_then(|picker| picker.source.as_ref())
        else {
            return false;
        };
        source.source_workspace_id.as_deref() == Some(target.id.as_str())
            && snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == target.id)
                .is_some_and(|workspace| {
                    workspace.branch == target.branch
                        && workspace.worktree.as_ref().is_some_and(|tree| {
                            !tree.is_linked_worktree
                                && tree.key == source.repo_key
                                && tree.label == source.repo_name
                        })
                })
    }

    pub(super) fn open_existing_worktrees(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search worktrees..."));
        let subscription = cx.subscribe(&search, |this, search, event: &InputEvent, cx| {
            if !matches!(event, InputEvent::Change) {
                return;
            }
            if let Some(picker) = &mut this.menu.worktree_open {
                picker.filter(&search.read(cx).value());
            }
            cx.notify();
        });
        let result = (|| {
            if !self.worktree_open_current() {
                return Err(crate::Error::StaleWorkspace);
            }
            let target = self
                .menu
                .target
                .as_ref()
                .ok_or(crate::Error::StaleWorkspace)?;
            self.endpoints[self.selected_endpoint]
                .connection
                .request_dialog(
                    &target.boot_id,
                    Method::WorktreeList,
                    serde_json::json!({"workspace_id": target.id, "trust_repository": false}),
                )
        })();
        search.update(cx, |search, cx| search.focus(window, cx));
        let mut picker = Picker::new(search);
        picker._subscription = Some(subscription);
        picker.pending = result.as_ref().ok().cloned();
        self.menu.worktree_open = Some(picker);
        self.menu.error = result.err().map(|error| error.to_string());
        cx.notify();
    }

    pub(super) fn render_existing_worktrees(
        &self,
        weak: &WeakEntity<HerdrWindow>,
        cx: &App,
    ) -> Div {
        let Some(picker) = &self.menu.worktree_open else {
            return div();
        };
        let muted = cx.theme().muted_foreground;
        let rows = (0..picker.filtered.len()).filter_map(|row| {
            let entry = picker.entry(row)?;
            let label = entry.branch.as_deref().unwrap_or(&entry.label).to_owned();
            let status = match (entry.open_workspace_id.is_some(), entry.is_detached) {
                (true, _) => "open",
                (_, true) => "detached",
                _ => "",
            };
            Some(
                ListItem::new(("open-worktree-row", row))
                    .selected(row == picker.selected)
                    .child(
                        v_flex()
                            .debug_selector(move || format!("open-worktree-row-{row}"))
                            .w_full()
                            .min_w_0()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .min_w_0()
                                    .child(div().flex_1().min_w_0().truncate().child(label))
                                    .when(!status.is_empty(), |line| {
                                        line.child(div().text_color(muted).child(status))
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(entry.path.clone()),
                            ),
                    )
                    .on_click(listener(weak, move |this, window, cx| {
                        if this.menu.page != Some(Page::Dialog(WorkspaceAction::OpenWorktree))
                            || this.menu.creation.is_some()
                        {
                            return;
                        }
                        if let Some(picker) = &mut this.menu.worktree_open {
                            picker.selected = row;
                        }
                        this.submit_workspace_dialog(window, cx);
                    })),
            )
        });
        v_flex()
            .gap_2()
            .child(
                div()
                    .debug_selector(|| "open-worktree-search".into())
                    .child(Input::new(&picker.search)),
            )
            .child(div().text_sm().text_color(muted).child(format!(
                "{} of {} worktrees",
                picker.filtered.len(),
                picker.entries.len()
            )))
            .child(
                v_flex()
                    .id("open-worktrees")
                    .h(px(300.))
                    .overflow_y_scroll()
                    .track_scroll(&picker.scroll)
                    .when(picker.filtered.is_empty(), |list| {
                        list.child(div().py_3().text_color(muted).child(
                            if picker.pending.is_some() {
                                "Loading worktrees..."
                            } else if !picker.entries.is_empty() {
                                "No matching worktrees. Try a shorter search."
                            } else if self.menu.error.is_some() {
                                "Worktrees unavailable."
                            } else {
                                "No Git worktrees found for this repository."
                            },
                        ))
                    })
                    .children(rows),
            )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn malformed_worktree_list_retains_parser_source() {
        use std::error::Error;
        let result = super::listing(&serde_json::json!({"result": {
            "type": "worktree_list", "source": {"repo_key":"key", "repo_name":"repo"},
            "worktrees": [{"path": "/checkout", "label": 42}]
        }}));
        let Err(error) = result else {
            panic!("invalid entry accepted")
        };
        assert!(matches!(error, crate::Error::WorktreeListDecode(_)));
        assert!(
            error
                .source()
                .is_some_and(|source| source.is::<serde_json::Error>())
        );
        assert_eq!(
            error.to_string(),
            "Malformed worktree list. Dismiss and reopen the menu."
        );
    }
}
