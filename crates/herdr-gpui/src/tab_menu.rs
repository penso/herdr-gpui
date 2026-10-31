// i18n: migrated
use crate::i18n::fl;
use crate::{
    HerdrWindow,
    browser::GroupId,
    controls::Command,
    menu::Page,
    notification_mutes::Scope,
    pane_move::{self, Choice, Destination},
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use herdr_client::{Method, protocol::ClientShellSnapshot};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Tells one tab move's result from another's, so a late answer never
/// settles a menu reopened since.
static NEXT_MOVE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
struct Target {
    boot: String,
    workspace: String,
    tab: String,
    label: String,
    /// The tab's panes in snapshot order, moved together to another workspace.
    panes: Vec<String>,
    /// Destinations the tab can move to: a new workspace, then other workspaces.
    choices: Vec<Choice>,
}

#[cfg(test)]
mod tests;

impl Target {
    fn capture(snapshot: &ClientShellSnapshot, id: &str) -> Option<Self> {
        let tab = snapshot.tabs.iter().find(|tab| tab.tab_id == id)?;
        let panes = snapshot
            .panes
            .iter()
            .filter(|pane| pane.tab_id == tab.tab_id && pane.workspace_id == tab.workspace_id)
            .map(|pane| pane.pane_id.clone())
            .collect();
        let choices = pane_move::workspace_choices(snapshot, &tab.workspace_id);
        Some(Self {
            boot: snapshot.boot_id.clone(),
            workspace: tab.workspace_id.clone(),
            tab: tab.tab_id.clone(),
            label: tab.label.clone(),
            panes,
            choices,
        })
    }

    fn validate(&self, snapshot: &ClientShellSnapshot) -> crate::Result<()> {
        if snapshot.boot_id != self.boot
            || !snapshot
                .workspaces
                .iter()
                .any(|w| w.workspace_id == self.workspace)
            || !snapshot
                .tabs
                .iter()
                .any(|t| t.tab_id == self.tab && t.workspace_id == self.workspace)
        {
            return Err(crate::Error::StaleTab);
        }
        Ok(())
    }

    fn validate_move(&self, snapshot: &ClientShellSnapshot) -> crate::Result<()> {
        self.validate(snapshot)?;
        let current = snapshot
            .panes
            .iter()
            .filter(|p| p.tab_id == self.tab && p.workspace_id == self.workspace);
        if self.panes.is_empty()
            || current.clone().count() != self.panes.len()
            || !self
                .panes
                .iter()
                .all(|id| current.clone().any(|p| p.pane_id == *id))
        {
            return Err(crate::Error::StaleTab);
        }
        Ok(())
    }

    fn rename_params(&self, label: &str) -> crate::Result<Value> {
        let label = label.trim();
        if label.is_empty() {
            return Err(crate::Error::EmptyTabName);
        }
        Ok(json!({"tab_id": self.tab, "label": label}))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    NewTab,
    Rename,
    MoveToWorkspace,
    /// Silence this tab's notifications, or let them through again.
    Mute,
    Unmute,
    /// The tab is silent because its workspace is muted: lift that.
    UnmuteWorkspace,
    Close,
}

impl Action {
    fn label(self) -> String {
        match self {
            Self::NewTab => fl!("tab-menu-new-tab"),
            Self::Rename => fl!("tab-menu-rename"),
            Self::MoveToWorkspace => fl!("tab-menu-move-to-workspace"),
            Self::Mute => fl!("tab-menu-mute"),
            Self::Unmute => fl!("tab-menu-unmute"),
            Self::UnmuteWorkspace => fl!("tab-menu-unmute-workspace"),
            Self::Close => fl!("tab-menu-close"),
        }
    }
}

/// Herdr's own tab menu, in its order.
const ACTIONS: [Action; 4] = [
    Action::NewTab,
    Action::Rename,
    Action::MoveToWorkspace,
    Action::Close,
];

impl TabMenu {
    /// The rows this menu offers: moving to a workspace needs a host the
    /// `herdr` CLI can run on and at least one pane in the tab. This
    /// client's mute row comes before closing, which stays last.
    fn actions(&self) -> Vec<Action> {
        let mut actions: Vec<_> = ACTIONS
            .into_iter()
            .filter(|action| match action {
                Action::MoveToWorkspace => self.movable && !self.target.panes.is_empty(),
                _ => true,
            })
            .collect();
        let close = actions
            .iter()
            .position(|action| *action == Action::Close)
            .unwrap_or(actions.len());
        actions.insert(close, self.mute);
        actions
    }

    /// How many rows the current page lists.
    fn rows(&self, page: Option<Page>) -> usize {
        if page == Some(Page::MoveTab) {
            self.target.choices.len()
        } else {
            self.actions().len()
        }
    }
}

pub(super) struct TabMenu {
    target: Target,
    /// The editor group whose strip opened the menu, if any. A new tab
    /// opens in it, and while split, closing removes the tab from it alone.
    group: Option<GroupId>,
    selected: Option<usize>,
    input: Option<Entity<SearchInput>>,
    pending: Option<String>,
    error: Option<String>,
    /// Whether this endpoint's host can run `herdr pane move`.
    movable: bool,
    /// The move in flight, which keeps the menu from starting another.
    moving: Option<u64>,
    /// The move picker's panel scroll, so a row the keyboard selects in a
    /// long list of workspaces scrolls into view.
    scroll: ScrollHandle,
    /// The mute row, as the tab and its workspace were muted when the menu
    /// opened.
    mute: Action,
}

impl HerdrWindow {
    pub(super) fn open_tab_menu(
        &mut self,
        id: &str,
        group: Option<GroupId>,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self
            .live
            .snapshot
            .as_ref()
            .and_then(|s| Target::capture(s, id))
        else {
            return;
        };
        if !self.open_menu(window, cx) {
            return;
        }
        let connect = &self.endpoints[self.selected_endpoint].connection.target;
        let movable = pane_move::refusal(connect).is_none();
        let mute = if self.muted_on_selected(Scope::Workspace, &target.workspace, cx) {
            Action::UnmuteWorkspace
        } else if self.muted_on_selected(Scope::Tab, &target.tab, cx) {
            Action::Unmute
        } else {
            Action::Mute
        };
        self.menu.anchor = anchor;
        self.menu.page = Some(Page::Tab);
        self.menu.tab = Some(TabMenu {
            target,
            group,
            selected: None,
            input: None,
            pending: None,
            error: None,
            movable,
            moving: None,
            scroll: ScrollHandle::new(),
            mute,
        });
    }

    /// Opens the focused tab's rename dialog, as its menu's "Rename" row
    /// would, just below the tab strip.
    pub(super) fn rename_focused_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self
            .live
            .snapshot
            .as_ref()
            .and_then(|s| s.focused_tab_id.clone())
        else {
            return;
        };
        self.open_tab_menu(&id, None, self.bounds.origin, window, cx);
        if self.menu.page == Some(Page::Tab) {
            self.activate_tab_menu(Action::Rename, window, cx);
        }
    }

    fn validate_tab_target(&self) -> crate::Result<&Target> {
        if !self.menu_target_current() {
            return Err(crate::Error::StaleConnection);
        }
        let target = &self.menu.tab.as_ref().ok_or(crate::Error::NoTab)?.target;
        target.validate(
            self.live
                .snapshot
                .as_ref()
                .ok_or(crate::Error::NotConnected)?,
        )?;
        Ok(target)
    }

    fn tab_error(&mut self, error: impl std::fmt::Display, cx: &mut Context<Self>) {
        if let Some(tab) = &mut self.menu.tab {
            tab.error = Some(error.to_string());
        }
        cx.notify();
    }

    fn activate_tab_menu(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .menu
            .tab
            .as_ref()
            .is_some_and(|tab| tab.moving.is_some())
        {
            return;
        }
        let target = match self.validate_tab_target() {
            Ok(target) => target.clone(),
            Err(error) => {
                self.tab_error(error, cx);
                return;
            }
        };
        let group = self.menu.tab.as_ref().and_then(|tab| tab.group);
        match action {
            Action::NewTab => {
                self.dismiss_menu(window, cx);
                if let Some(group) = group {
                    self.expect_new_tab_in(group);
                }
                self.command(Command::Tab, window, cx);
            }
            // As the tab's own close button: the close confirmation decides.
            Action::Close => self.open_tab_close(&target.tab, window, cx),
            Action::Mute | Action::Unmute | Action::UnmuteWorkspace => {
                let (scope, id) = match action {
                    Action::UnmuteWorkspace => (Scope::Workspace, &target.workspace),
                    _ => (Scope::Tab, &target.tab),
                };
                self.set_muted_on_selected(scope, id, &target.boot, action == Action::Mute, cx);
                self.dismiss_menu(window, cx);
            }
            Action::MoveToWorkspace => {
                if let Some(tab) = &mut self.menu.tab {
                    tab.selected = None;
                    tab.error = None;
                }
                self.menu.page = Some(Page::MoveTab);
                cx.notify();
            }
            Action::Rename => {
                let input = cx.new(SearchInput::new);
                input.update(cx, |input, cx| {
                    input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
                    input.set_placeholder_with(|| fl!("tab-menu-rename-placeholder"), cx);
                    input.set_text_selected(&target.label, cx);
                    window.focus(&input.focus, cx);
                });
                if let Some(tab) = &mut self.menu.tab {
                    tab.input = Some(input);
                    tab.error = None;
                }
                self.menu.page = Some(Page::RenameTab);
                cx.notify();
            }
        }
    }

    /// Moves every pane in the menu's tab to `destination` through the `herdr`
    /// CLI on a background thread. The menu stays open until Herdr answers.
    pub(super) fn move_tab(
        &mut self,
        destination: Destination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .menu
            .tab
            .as_ref()
            .is_none_or(|tab| tab.moving.is_some())
        {
            return;
        }
        let started = (|| {
            let target = self.validate_tab_target()?;
            let snapshot = self
                .live
                .snapshot
                .as_ref()
                .ok_or(crate::Error::NotConnected)?;
            target.validate_move(snapshot)?;
            if !destination.exists(snapshot) {
                return Err(match destination {
                    Destination::NewTab { .. } => crate::Error::StaleWorkspace,
                    Destination::Tab { .. } | Destination::NewWorkspace => crate::Error::StaleTab,
                });
            }
            let label = snapshot
                .tabs
                .iter()
                .find(|t| t.tab_id == target.tab && t.workspace_id == target.workspace)
                .map(|t| t.label.as_str())
                .unwrap_or(target.label.as_str());
            let label = (!label.trim().is_empty()).then(|| label.to_owned());
            let request = pane_move::TabRequest::new(target.panes.clone(), destination, label)?;
            let host =
                pane_move::host_for(&self.endpoints[self.selected_endpoint].connection.target)?;
            Ok::<_, crate::Error>((request, host))
        })();
        let (request, host) = match started {
            Ok(started) => started,
            Err(error) => {
                self.tab_error(error, cx);
                return;
            }
        };
        let id = NEXT_MOVE.fetch_add(1, Ordering::Relaxed);
        if let Some(tab) = &mut self.menu.tab {
            tab.moving = Some(id);
            tab.error = None;
        }
        // The CLI is a child process, possibly over SSH: never on this thread.
        let job = cx
            .background_executor()
            .spawn(async move { request.run(&host, &AtomicBool::new(false)) });
        cx.spawn_in(window, async move |this, cx| {
            let result = job.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_tab_move(id, result, window, cx)
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn finish_tab_move(
        &mut self,
        id: u64,
        result: pane_move::Result<pane_move::TabMoved>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self
            .menu
            .tab
            .as_ref()
            .is_some_and(|tab| tab.moving == Some(id));
        match result {
            Ok(moved) => {
                if open {
                    self.dismiss_menu(window, cx);
                }
                // The panes all moved, so the menu closes either way; a lost
                // name is reported on the window rather than in a closed menu.
                if let Some(warning) = moved.warning {
                    self.local_error = Some(warning.to_string());
                    cx.notify();
                }
            }
            Err(error) if open => {
                if let Some(tab) = &mut self.menu.tab {
                    tab.moving = None;
                }
                self.tab_error(error, cx);
            }
            Err(error) => {
                self.local_error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    fn submit_tab_move_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(destination) = self
            .menu
            .tab
            .as_ref()
            .and_then(|tab| tab.selected.and_then(|index| tab.target.choices.get(index)))
            .map(|choice| choice.destination.clone())
        else {
            return;
        };
        self.move_tab(destination, window, cx);
    }

    fn submit_tab_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = &self.menu.tab else { return };
        let Some(input) = &tab.input else { return };
        if tab.pending.is_some() || input.read(cx).is_composing() {
            return;
        }
        let result = (|| {
            let target = self.validate_tab_target()?;
            let params = target.rename_params(input.read(cx).text())?;
            if !self.input_ready() {
                return Err(crate::Error::ConnectionNotReady);
            }
            let endpoint = &self.endpoints[self.selected_endpoint];
            let handle = endpoint
                .connection
                .handle
                .as_ref()
                .ok_or(crate::Error::NotConnected)?;
            let mut inbox = endpoint
                .connection
                .inbox
                .try_lock()
                .map_err(|_| crate::Error::ConnectionBusy)?;
            // Install correlation under the same lock used by the event reducer.
            let request = handle.request(&target.boot, Method::TabRename, params)?;
            inbox.tab_rename = Some(crate::state::RenameResult {
                request: request.clone(),
                result: None,
            });
            Ok::<_, crate::Error>(request)
        })();
        match result {
            Ok(request) => {
                if let Some(tab) = &mut self.menu.tab {
                    tab.pending = Some(request);
                    tab.error = None;
                }
                window.focus(&self.menu.focus, cx);
                cx.notify();
            }
            Err(error) => self.tab_error(error, cx),
        }
    }

    pub(super) fn poll_tab_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.menu.tab.as_ref().and_then(|tab| tab.pending.as_ref()) else {
            return;
        };
        let result = if let Err(error) = self.validate_tab_target() {
            Some(Err(std::sync::Arc::new(error)))
        } else {
            self.live
                .tab_rename
                .as_ref()
                .filter(|r| &r.request == request)
                .and_then(|r| r.result.clone())
        };
        match result {
            Some(Ok(())) => self.dismiss_menu(window, cx),
            Some(Err(error)) => {
                if let Some(tab) = &mut self.menu.tab {
                    tab.pending = None;
                    if let Some(input) = &tab.input {
                        window.focus(&input.read(cx).focus.clone(), cx);
                    }
                }
                self.tab_error(error, cx);
            }
            None => {}
        }
    }

    pub(super) fn tab_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = &mut self.menu.tab else {
            return;
        };
        let key = event.keystroke.key.as_str();
        if self.menu.page == Some(Page::RenameTab)
            && tab.pending.is_none()
            && (tab
                .input
                .as_ref()
                .is_some_and(|input| input.read(cx).is_composing())
                || !matches!(key, "escape" | "enter"))
        {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        match key {
            "escape" => self.dismiss_menu(window, cx),
            "enter" if self.menu.page == Some(Page::RenameTab) => {
                self.submit_tab_rename(window, cx)
            }
            "up" | "down" if matches!(self.menu.page, Some(Page::Tab | Page::MoveTab)) => {
                let count = tab.rows(self.menu.page);
                if count == 0 {
                    return;
                }
                tab.selected = Some(match (tab.selected, key) {
                    (None, "up") => count - 1,
                    (None, _) => 0,
                    (Some(i), "up") => (i + count - 1) % count,
                    (Some(i), _) => (i + 1) % count,
                });
                // The picker's heading is the panel's first child.
                if self.menu.page == Some(Page::MoveTab)
                    && let Some(selected) = tab.selected
                {
                    tab.scroll.scroll_to_item(selected + 1);
                }
                cx.notify();
            }
            "enter" if self.menu.page == Some(Page::MoveTab) => {
                self.submit_tab_move_picker(window, cx)
            }
            "enter" => {
                if let Some(action) = tab
                    .selected
                    .and_then(|index| tab.actions().get(index).copied())
                {
                    self.activate_tab_menu(action, window, cx);
                }
            }
            _ => {}
        }
    }

    fn render_tab_move_pending(&self) -> Div {
        div().flex().flex_col().child(
            div()
                .debug_selector(|| "tab-move-pending".into())
                .p(px(8.))
                .text_color(rgb(self.theme.muted))
                .child(fl!("tab-menu-moving")),
        )
    }

    /// Fills the menu's `panel` with the workspace picker. The heading and
    /// rows are the panel's own children, which its scroll handle addresses by
    /// index, so a row the keyboard selects in a long list scrolls into view.
    pub(crate) fn render_tab_move_picker(
        &self,
        panel: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let Some(tab) = &self.menu.tab else {
            return panel;
        };
        if tab.moving.is_some() {
            return panel.child(self.render_tab_move_pending());
        }
        let mut body = panel.track_scroll(&tab.scroll).flex().flex_col().child(
            div()
                .flex_none()
                .px(px(8.))
                .py(px(4.))
                .text_color(rgb(self.theme.muted))
                .child(fl!("tab-menu-move-heading")),
        );
        for (index, choice) in tab.target.choices.iter().enumerate() {
            let destination = choice.destination.clone();
            let place = choice.place.clone();
            body = body.child(
                div()
                    .id(("tab-move-destination", index))
                    .flex_none()
                    .debug_selector(move || format!("tab-move-{index}"))
                    .min_h(px(self.config.ui.line_height() + 12.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .cursor_pointer()
                    .when(tab.selected == Some(index), |row| {
                        row.bg(rgb(self.theme.active))
                    })
                    .hover(|row| row.bg(rgb(self.theme.active)))
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered && let Some(tab) = &mut this.menu.tab {
                            tab.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .debug_selector(move || format!("tab-move-place-{index}"))
                            .min_w(px(0.))
                            .truncate()
                            .child(place),
                    )
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        // The picker replaces the menu under the pointer, and
                        // snapping it into the window can shift its rows, so
                        // a double-click's second click would otherwise pick
                        // whatever row it lands on.
                        if event.click_count() > 1 {
                            return;
                        }
                        this.move_tab(destination.clone(), window, cx)
                    })),
            );
        }
        body.when_some(tab.error.clone(), |body, error| {
            body.child(div().flex_none().p(px(8.)).child(error))
        })
    }

    pub(super) fn render_tab_menu(&self, cx: &mut Context<Self>) -> Div {
        let Some(tab) = &self.menu.tab else {
            return div();
        };
        let theme = &self.theme;
        let mut body = div().flex().flex_col();
        if tab.moving.is_some() {
            body = self.render_tab_move_pending();
        } else if self.menu.page == Some(Page::Tab) {
            for (index, action) in tab.actions().into_iter().enumerate() {
                body = body.child(
                    div()
                        .id(("tab-menu-action", index))
                        .debug_selector(move || format!("tab-menu-{index}"))
                        .min_h(px(self.config.ui.line_height() + 12.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .when(tab.selected == Some(index), |row| row.bg(rgb(theme.active)))
                        .hover(|row| row.bg(rgb(theme.active)))
                        .child(
                            div()
                                .debug_selector(move || format!("tab-menu-label-{index}"))
                                .min_w_0()
                                .truncate()
                                .child(action.label()),
                        )
                        .on_hover(cx.listener(move |this, hovered, _, cx| {
                            if *hovered && let Some(tab) = &mut this.menu.tab {
                                tab.selected = Some(index);
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.activate_tab_menu(action, window, cx)
                        })),
                );
            }
        } else {
            body =
                body.p(px(8.))
                    .gap(px(12.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(fl!("tab-menu-rename-title")),
                    )
                    .when_some(tab.input.clone(), |body, input| {
                        if tab.pending.is_some() {
                            body.child(div().child(input.read(cx).text().to_owned()))
                        } else {
                            body.child(input)
                        }
                    })
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                div()
                                    .id("tab-rename-cancel")
                                    .p(px(6.))
                                    .cursor_pointer()
                                    .child(fl!("tab-menu-cancel"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.dismiss_menu(window, cx)
                                    })),
                            )
                            .child(
                                div()
                                    .id("tab-rename-submit")
                                    .p(px(6.))
                                    .rounded(px(crate::config::corners::CONTROL))
                                    .bg(rgb(theme.active))
                                    .cursor_pointer()
                                    .child(if tab.pending.is_some() {
                                        fl!("tab-menu-renaming")
                                    } else {
                                        fl!("tab-menu-rename-submit")
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.submit_tab_rename(window, cx)
                                    })),
                            ),
                    );
        }
        body.when_some(tab.error.clone(), |body, error| {
            // No width of its own, so the labels set the panel's width and
            // the message wraps inside it.
            body.child(div().w_0().min_w_full().p(px(8.)).child(error))
        })
    }
}
