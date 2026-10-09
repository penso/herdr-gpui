//! Moving a workspace's VS Code tab between its panel and its editor
//! groups. In the groups it is listed in the strips and shown as any page
//! is, while staying the workspace's one VS Code tab: it keeps its page, its
//! server check, and its token, and no agent ever reuses it.
use super::{
    Location, Store, TabId,
    groups::{Pick, Shown},
    store::Place,
    view::store,
};
use crate::{HerdrWindow, window::Flash};
use gpui::{prelude::*, *};

impl HerdrWindow {
    /// The focused workspace's VS Code tab, while it is in the groups.
    pub(super) fn grouped_code_tab(&self, cx: &App) -> Option<TabId> {
        let (scope, workspace) = self.browser_key()?;
        store(cx)?
            .code_tab(&scope, &workspace)
            .filter(|tab| tab.place == Place::CodeGroup)
            .map(|tab| tab.id)
    }

    /// Whether some group of the focused workspace shows the page `id`.
    pub(super) fn group_shows_page(&self, id: TabId, cx: &App) -> bool {
        self.group_slots()
            .into_iter()
            .any(|slot| self.group_shown(slot.id, cx) == Shown::Page(id))
    }

    /// Moves the focused workspace's VS Code tab from its panel into a new
    /// group, split off to the right of the group in use. The page moves
    /// with it, keeping its state; a workspace without a VS Code tab yet
    /// gets one.
    pub(crate) fn move_code_to_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((scope, workspace)) = self.browser_key() else {
            self.show_flash(Flash::warning("Open a workspace first"), cx);
            return;
        };
        if self.grouped_code_tab(cx).is_some() {
            self.show_flash(Flash::warning("VS Code is already in a group"), cx);
            return;
        }
        let existing = store(cx)
            .and_then(|store| store.code_tab(&scope, &workspace))
            .map(|tab| tab.id);
        let id = match (existing, self.config.code.url.clone()) {
            (Some(id), _) => Some(id),
            (None, Some(url)) if super::EMBEDDED => {
                let start = self.code_start(&url);
                Store::update(cx, |store| {
                    store.open_code_tab(scope, &workspace, Location::Web { url: start })
                })
            }
            (None, _) => {
                self.show_flash(Flash::warning("Set the VS Code server in Settings"), cx);
                return;
            }
        };
        let Some(id) = id else {
            self.show_flash(Flash::warning("Too many browser tabs are open"), cx);
            return;
        };
        Store::update(cx, |store| store.move_code_tab(id, Place::CodeGroup));
        #[cfg(any(target_os = "macos", windows))]
        self.browser.pages.blur(id, cx);
        let Some(layout) = self.ensure_layout() else {
            return;
        };
        layout.code = false;
        let active = layout.active();
        // The new group to the right becomes the one in use.
        self.split_group(active, window, cx);
        let group = self.ensure_layout().map(|layout| layout.active());
        self.show_browser_tab_in(group, id, window, cx);
    }

    /// Moves the focused workspace's VS Code tab from the groups back into
    /// its panel, which shows. The groups that showed it show their
    /// terminal again; its page moves with it, keeping its state.
    pub(crate) fn move_code_to_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.grouped_code_tab(cx) else {
            self.show_flash(Flash::warning("VS Code is not in a group"), cx);
            return;
        };
        Store::update(cx, |store| store.move_code_tab(id, Place::Code));
        let focused = self.focused_herdr_tab().map(str::to_owned);
        let Some(layout) = self.ensure_layout() else {
            return;
        };
        layout.replace(&Pick::Page(id), None, focused.as_deref());
        layout.code = true;
        self.sync_addresses(true, window, cx);
        self.ensure_code_page(window, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }
}
