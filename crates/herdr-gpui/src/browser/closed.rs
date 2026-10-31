//! Recently closed tabs of each workspace, both browser pages and Herdr
//! terminal tabs, in the order they were closed, so Reopen Closed Tab
//! (`Cmd-Shift-T` / `Ctrl-Alt-Shift-T`) brings back the most recent one.
//!
//! Browser tabs keep their address, title, origin pane, and kind in the
//! [`Store`]; terminal tabs keep their workspace, custom label, and working
//! directory so `tab.create` opens a fresh shell where the closed one stood.

use super::{
    Location, Pick, Scope, Store, Tab, TabId,
    view::{scope, store},
};
use crate::i18n::fl;
use crate::{HerdrWindow, window::Flash};
use gpui::{Context, Window};
use herdr_client::{
    Method,
    protocol::{ClientShellPane, ClientShellSnapshot, ClientShellTab},
};
use serde_json::{Value, json};
use std::collections::HashSet;

const MAX_CLOSED_TABS: usize = 32;
const MAX_CLOSED_WORKSPACES: usize = 64;

/// What a closed Herdr terminal tab needs to open a replacement shell with
/// `tab.create`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClosedHerdrTab {
    pub(crate) tab_id: String,
    pub(crate) workspace_id: String,
    pub(crate) label: Option<String>,
    pub(crate) cwd: Option<String>,
}

impl ClosedHerdrTab {
    pub(crate) fn capture(snapshot: &ClientShellSnapshot, tab_id: &str) -> Option<Self> {
        let tab = snapshot.tabs.iter().find(|tab| tab.tab_id == tab_id)?;
        Some(Self::from_tab(snapshot, tab))
    }

    pub(crate) fn from_tab(snapshot: &ClientShellSnapshot, tab: &ClientShellTab) -> Self {
        let label = tab
            .custom_label
            .then(|| tab.label.trim().to_owned())
            .filter(|label| !label.is_empty());
        let pane_cwd = |pane: &ClientShellPane| {
            pane.foreground_cwd
                .as_deref()
                .filter(|path| !path.is_empty())
                .or_else(|| pane.cwd.as_deref().filter(|path| !path.is_empty()))
                .map(str::to_owned)
        };
        let cwd = snapshot
            .panes
            .iter()
            .find(|pane| {
                pane.tab_id == tab.tab_id && pane.workspace_id == tab.workspace_id && pane.focused
            })
            .and_then(pane_cwd)
            .or_else(|| {
                snapshot
                    .panes
                    .iter()
                    .filter(|pane| {
                        pane.tab_id == tab.tab_id && pane.workspace_id == tab.workspace_id
                    })
                    .find_map(pane_cwd)
            });
        Self {
            tab_id: tab.tab_id.clone(),
            workspace_id: tab.workspace_id.clone(),
            label,
            cwd,
        }
    }

    pub(crate) fn create_params(&self) -> Value {
        let mut params = json!({
            "workspace_id": self.workspace_id,
            "focus": true,
        });
        if let Some(cwd) = &self.cwd {
            params["cwd"] = Value::String(cwd.clone());
        }
        if let Some(label) = &self.label {
            params["label"] = Value::String(label.clone());
        }
        params
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClosedTab {
    Page(Tab),
    Herdr(ClosedHerdrTab),
}

const MAX_RECORDED_HERDR: usize = 256;

#[derive(Default)]
pub(super) struct ClosedTabs {
    workspaces: Vec<((Scope, String), Vec<ClosedTab>)>,
    recorded_herdr: Vec<((Scope, String), Vec<String>)>,
}

impl ClosedTabs {
    fn workspace_mut(&mut self, key: (Scope, String)) -> &mut Vec<ClosedTab> {
        if let Some(index) = self
            .workspaces
            .iter()
            .position(|(existing, _)| *existing == key)
        {
            let entry = self.workspaces.remove(index);
            self.workspaces.push(entry);
        } else {
            if self.workspaces.len() >= MAX_CLOSED_WORKSPACES {
                self.workspaces.remove(0);
            }
            self.workspaces.push((key, Vec::new()));
        }
        let last = self.workspaces.len() - 1;
        &mut self.workspaces[last].1
    }

    fn recorded_herdr_mut(&mut self, key: (Scope, String)) -> &mut Vec<String> {
        if let Some(index) = self
            .recorded_herdr
            .iter()
            .position(|(existing, _)| *existing == key)
        {
            let entry = self.recorded_herdr.remove(index);
            self.recorded_herdr.push(entry);
        } else {
            if self.recorded_herdr.len() >= MAX_CLOSED_WORKSPACES {
                self.recorded_herdr.remove(0);
            }
            self.recorded_herdr.push((key, Vec::new()));
        }
        let last = self.recorded_herdr.len() - 1;
        &mut self.recorded_herdr[last].1
    }

    pub(super) fn push_page(&mut self, tab: Tab) {
        let key = (tab.scope.clone(), tab.workspace_id.clone());
        let entries = self.workspace_mut(key);
        entries.retain(|entry| match entry {
            ClosedTab::Page(earlier) => {
                earlier.id != tab.id && (!tab.place.is_editor() || earlier.place != tab.place)
            }
            ClosedTab::Herdr(_) => true,
        });
        if entries.len() >= MAX_CLOSED_TABS {
            entries.remove(0);
        }
        entries.push(ClosedTab::Page(tab));
    }

    pub(super) fn push_herdr(&mut self, scope: Scope, tab: ClosedHerdrTab) {
        let key = (scope, tab.workspace_id.clone());
        let recorded = self.recorded_herdr_mut(key.clone());
        if recorded.contains(&tab.tab_id) {
            return;
        }
        if recorded.len() >= MAX_RECORDED_HERDR {
            recorded.remove(0);
        }
        recorded.push(tab.tab_id.clone());
        self.push_herdr_entry(key, tab);
    }

    /// Puts back a terminal tab whose reopen could not be sent. It was
    /// recorded when it closed, so the cross-window guard in [`Self::push_herdr`]
    /// would otherwise drop it.
    pub(super) fn restore_herdr(&mut self, scope: Scope, tab: ClosedHerdrTab) {
        let key = (scope, tab.workspace_id.clone());
        let recorded = self.recorded_herdr_mut(key.clone());
        if !recorded.contains(&tab.tab_id) {
            if recorded.len() >= MAX_RECORDED_HERDR {
                recorded.remove(0);
            }
            recorded.push(tab.tab_id.clone());
        }
        self.push_herdr_entry(key, tab);
    }

    fn push_herdr_entry(&mut self, key: (Scope, String), tab: ClosedHerdrTab) {
        let entries = self.workspace_mut(key);
        if entries
            .iter()
            .any(|entry| matches!(entry, ClosedTab::Herdr(earlier) if earlier.tab_id == tab.tab_id))
        {
            return;
        }
        if entries.len() >= MAX_CLOSED_TABS {
            entries.remove(0);
        }
        entries.push(ClosedTab::Herdr(tab));
    }

    pub(super) fn peek(&self, scope: &Scope, workspace_id: &str) -> Option<&ClosedTab> {
        self.workspaces
            .iter()
            .find(|((saved_scope, saved_workspace), _)| {
                saved_scope == scope && saved_workspace == workspace_id
            })
            .and_then(|(_, entries)| entries.last())
    }

    pub(super) fn pop(&mut self, scope: &Scope, workspace_id: &str) -> Option<ClosedTab> {
        let index = self
            .workspaces
            .iter()
            .position(|((saved_scope, saved_workspace), _)| {
                saved_scope == scope && saved_workspace == workspace_id
            })?;
        let popped = self.workspaces[index].1.pop();
        if self.workspaces[index].1.is_empty() {
            self.workspaces.remove(index);
        }
        popped
    }

    pub(super) fn forget_workspaces(&mut self, scope: &Scope, closed: &[String]) {
        self.workspaces
            .retain(|((saved, workspace), _)| saved != scope || !closed.contains(workspace));
        self.recorded_herdr
            .retain(|((saved, workspace), _)| saved != scope || !closed.contains(workspace));
    }

    pub(super) fn has_workspaces(&self, scope: &Scope, closed: &[String]) -> bool {
        self.workspaces
            .iter()
            .any(|((saved, workspace), _)| saved == scope && closed.contains(workspace))
    }
}

impl Store {
    pub(crate) fn record_closed_page(&mut self, tab: Tab) {
        self.closed.push_page(tab);
    }

    pub(crate) fn record_closed_herdr_tab(&mut self, scope: Scope, tab: ClosedHerdrTab) {
        self.closed.push_herdr(scope, tab);
    }

    pub(crate) fn restore_closed_herdr_tab(&mut self, scope: Scope, tab: ClosedHerdrTab) {
        self.closed.restore_herdr(scope, tab);
    }

    pub(crate) fn peek_closed(&self, scope: &Scope, workspace_id: &str) -> Option<&ClosedTab> {
        self.closed.peek(scope, workspace_id)
    }

    pub(crate) fn pop_closed(&mut self, scope: &Scope, workspace_id: &str) -> Option<ClosedTab> {
        self.closed.pop(scope, workspace_id)
    }

    /// Reopens a closed browser tab in its workspace, reusing a singleton tab
    /// when the workspace already has one open.
    pub(crate) fn reopen_page(&mut self, tab: Tab) -> Option<TabId> {
        if let Some(editor) = tab.place.editor()
            && let Some(existing) = self.editor_tab(&tab.scope, &tab.workspace_id, editor)
        {
            return Some(existing.id);
        }
        if let Some(existing) = match &tab.location {
            Some(Location::Orchestrator { .. }) => self
                .in_workspace(&tab.scope, &tab.workspace_id)
                .find(|open| matches!(open.location, Some(Location::Orchestrator { .. })))
                .map(|open| open.id),
            Some(Location::Devices) => self
                .in_workspace(&tab.scope, &tab.workspace_id)
                .find(|open| matches!(open.location, Some(Location::Devices)))
                .map(|open| open.id),
            Some(Location::Review { checkout }) => self
                .in_workspace(&tab.scope, &tab.workspace_id)
                .find(|open| {
                    matches!(&open.location, Some(Location::Review { checkout: existing }) if existing == checkout)
                })
                .map(|open| open.id),
            Some(Location::Code { file }) => self
                .in_workspace(&tab.scope, &tab.workspace_id)
                .find(|open| {
                    matches!(&open.location, Some(Location::Code { file: existing }) if existing.path == file.path)
                })
                .map(|open| open.id),
            _ => None,
        } {
            return Some(existing);
        }
        self.restore_tab(tab)
    }
}

impl HerdrWindow {
    /// Remembers a Herdr tab whose close was just sent to the daemon so it can
    /// be reopened right away and in the order it was closed.
    pub(crate) fn record_closed_herdr_tab(&mut self, tab: ClosedHerdrTab, cx: &mut Context<Self>) {
        let scope = scope(&self.endpoints[self.selected_endpoint]);
        if let Some((seen_scope, _, _, tabs)) = &mut self.browser.herdr_tabs
            && *seen_scope == scope
        {
            tabs.retain(|seen| seen.tab_id != tab.tab_id);
        }
        Store::update(cx, |store| store.record_closed_herdr_tab(scope, tab));
    }

    /// Notices Herdr tabs that disappeared from the selected daemon's snapshot
    /// while their workspace stayed open, such as when a shell exits.
    pub(super) fn observe_closed_herdr_tabs(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.live.snapshot.as_ref() else {
            return;
        };
        let scope = scope(&self.endpoints[self.selected_endpoint]);
        let unchanged =
            self.browser
                .herdr_tabs
                .as_ref()
                .is_some_and(|(seen_scope, boot, revision, _)| {
                    seen_scope == &scope
                        && boot == &snapshot.boot_id
                        && *revision == snapshot.revision
                });
        if unchanged {
            return;
        }
        let current: Vec<ClosedHerdrTab> = snapshot
            .tabs
            .iter()
            .map(|tab| ClosedHerdrTab::from_tab(snapshot, tab))
            .collect();
        let previous = self.browser.herdr_tabs.replace((
            scope.clone(),
            snapshot.boot_id.clone(),
            snapshot.revision,
            current,
        ));
        let Some((_, _, _, seen)) = previous
            .filter(|(seen_scope, boot, _, _)| seen_scope == &scope && boot == &snapshot.boot_id)
        else {
            return;
        };
        let open_tabs: HashSet<&str> = snapshot
            .tabs
            .iter()
            .map(|tab| tab.tab_id.as_str())
            .collect();
        let open_workspaces: HashSet<&str> = snapshot
            .workspaces
            .iter()
            .map(|workspace| workspace.workspace_id.as_str())
            .collect();
        let closed: Vec<ClosedHerdrTab> = seen
            .into_iter()
            .filter(|tab| {
                !open_tabs.contains(tab.tab_id.as_str())
                    && open_workspaces.contains(tab.workspace_id.as_str())
            })
            .collect();
        if closed.is_empty() {
            return;
        }
        Store::update(cx, |store| {
            for tab in closed {
                store.record_closed_herdr_tab(scope.clone(), tab);
            }
        });
    }

    /// Reopens the most recently closed tab of the focused workspace, whether
    /// it was a browser tab or a Herdr terminal tab.
    pub(crate) fn reopen_closed_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((scope, workspace)) = self.browser_key() else {
            return;
        };
        let Some(closed) = store(cx)
            .and_then(|store| store.peek_closed(&scope, &workspace))
            .cloned()
        else {
            return;
        };
        match closed {
            ClosedTab::Page(_) => {
                let Some(ClosedTab::Page(page)) =
                    Store::update(cx, |store| store.pop_closed(&scope, &workspace))
                else {
                    return;
                };
                let Some(id) = Store::update(cx, |store| store.reopen_page(page.clone())) else {
                    Store::update(cx, |store| store.record_closed_page(page));
                    self.show_flash(Flash::warning(fl!("browser-too-many-tabs")), cx);
                    return;
                };
                self.poll_reviews(cx);
                self.poll_code_views(cx);
                self.poll_orchestrators(window, cx);
                self.poll_devices_overview(cx);
                match self.ensure_layout().map(|layout| layout.active()) {
                    Some(group) => self.move_tab_to_group(Pick::Page(id), group, window, cx),
                    None => self.show_browser_tab(id, window, cx),
                }
                if let Some(review) = self.reviews.get(&id) {
                    window.focus(review.focus(), cx);
                } else if let Some(view) = self.code_views.get(&id) {
                    window.focus(&view.focus, cx);
                } else if let Some(page) = self.devices_overview.pages.get(&id) {
                    window.focus(&page.focus, cx);
                }
            }
            ClosedTab::Herdr(_) => {
                if self.activation_deadline.is_some()
                    || !self.endpoints[self.selected_endpoint].surface_requested()
                    || self.endpoints[self.selected_endpoint]
                        .connection
                        .handle
                        .is_none()
                    || !self.live.snapshot.as_ref().is_some_and(|snapshot| {
                        snapshot
                            .workspaces
                            .iter()
                            .any(|candidate| candidate.workspace_id == workspace)
                    })
                {
                    return;
                }
                let Some(ClosedTab::Herdr(tab)) =
                    Store::update(cx, |store| store.pop_closed(&scope, &workspace))
                else {
                    return;
                };
                let previous_group = self.browser.new_tab_group.clone();
                if let Some(group) = self.active_group() {
                    self.expect_new_tab_in(group);
                }
                let params = tab.create_params();
                let sent =
                    self.request_focus_change(Method::TabCreate.as_str(), None, |handle, boot| {
                        handle.request(boot, Method::TabCreate, params)
                    });
                if !sent {
                    self.browser.new_tab_group = previous_group;
                    Store::update(cx, |store| store.restore_closed_herdr_tab(scope, tab));
                    return;
                }
                self.discard_composition(cx);
                window.focus(&self.focus, cx);
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests;
