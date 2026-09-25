//! The overlay shell: kit popup menus and kit dialogs, opened and dismissed on
//! behalf of the window. `MenuState::page` stays the one record of what is
//! open, so input fencing elsewhere keeps reading a single field; the kit owns
//! focus trapping, keyboard navigation, and the surfaces themselves.

use super::{Page, state::Popup};
use crate::{HerdrWindow, actions};
use gpui_kit::{
    component::{
        ActiveTheme as _, IconName, Root, WindowExt as _,
        alert::Alert,
        button::{Button, ButtonVariants as _},
        dialog::Dialog,
        h_flex,
        input::{self, InputState},
        menu::{PopupMenu, PopupMenuItem},
        v_flex,
    },
    prelude::*,
    *,
};
use herdr_client::Method;

/// Hosts the open popup menu and the kit dialog layer. It is a child view of
/// the window, so it renders after the window's own render has returned and
/// dialog builders may read the window's state.
pub(crate) struct OverlayLayer {
    window: WeakEntity<HerdrWindow>,
}

impl OverlayLayer {
    pub(super) fn new(window: WeakEntity<HerdrWindow>) -> Self {
        Self { window }
    }
}

/// Re-sends an Edit menu item to the focused kit input as the kit's own
/// editing action, so the menu bar and the field's shortcut behave the same.
fn forward_edit(action: Box<dyn Action>, window: &mut Window, cx: &mut App) {
    if window.has_focused_input(cx) {
        window.dispatch_action(action, cx);
    } else {
        cx.propagate();
    }
}

impl Render for OverlayLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let popup = self.window.upgrade().and_then(|view| {
            let view = view.read(cx);
            view.menu.popup.as_ref().map(|popup| {
                (
                    popup.menu.clone(),
                    popup.anchor,
                    popup.corner,
                    view.hover_menu.is_some(),
                )
            })
        });
        let weak = self.window.clone();
        div()
            .id("menu-overlay")
            .on_action(|_: &actions::Cut, window, cx| {
                forward_edit(Box::new(input::Cut), window, cx)
            })
            .on_action(|_: &actions::Copy, window, cx| {
                forward_edit(Box::new(input::Copy), window, cx)
            })
            .on_action(|_: &actions::Paste, window, cx| {
                forward_edit(Box::new(input::Paste), window, cx)
            })
            .on_action(|_: &actions::SelectAll, window, cx| {
                forward_edit(Box::new(input::SelectAll), window, cx)
            })
            .children(popup.map(|(menu, anchor, corner, hover)| {
                deferred(
                    anchored()
                        .position(anchor)
                        .anchor(corner)
                        .snap_to_window_with_margin(px(8.))
                        .child(
                            div()
                                .id("menu-panel")
                                .debug_selector(|| "menu-panel".into())
                                // A menu the pointer opened follows the pointer's
                                // own report of whether it is over the popup.
                                .when(hover, |panel| {
                                    panel.on_hover(move |hovered, _, cx| {
                                        let _ = weak.update(cx, |this, _| {
                                            if let Some(open) = &mut this.hover_menu {
                                                open.inside = *hovered;
                                            }
                                        });
                                    })
                                })
                                .child(menu),
                        ),
                )
                .with_priority(base::POPUP_PRIORITY)
            }))
            .children(Root::render_dialog_layer(window, cx))
    }
}

/// A kit click handler that runs `f` on the window, for popups and dialogs
/// whose builders only hold a weak handle.
pub(crate) fn listener<E: ?Sized>(
    weak: &WeakEntity<HerdrWindow>,
    f: impl Fn(&mut HerdrWindow, &mut Window, &mut Context<HerdrWindow>) + 'static,
) -> impl Fn(&E, &mut Window, &mut App) + 'static {
    let weak = weak.clone();
    move |_, window, cx| {
        let _ = weak.update(cx, |this, cx| f(this, window, cx));
    }
}

/// A dialog's Confirm handler (Enter, or its OK action) that runs `f` and
/// leaves closing to `f`: returning true would let the kit pop whatever dialog
/// is on top by then, which may be one `f` just opened.
pub(crate) fn submit(
    weak: &WeakEntity<HerdrWindow>,
    f: impl Fn(&mut HerdrWindow, &mut Window, &mut Context<HerdrWindow>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static {
    let weak = weak.clone();
    move |_, window, cx| {
        let _ = weak.update(cx, |this, cx| f(this, window, cx));
        false
    }
}

/// A dialog's error line, when it has one.
pub(crate) fn error_alert(id: &'static str, error: Option<&String>) -> Option<Alert> {
    error.map(|error| Alert::error(id, error.clone()))
}

/// A dialog's Cancel button and its primary action, as the footer's trailing pair.
pub(crate) fn dialog_buttons(
    weak: &WeakEntity<HerdrWindow>,
    confirm: Option<Button>,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .justify_end()
        .gap_2()
        .child(
            Button::new("dialog-cancel")
                .label("Cancel")
                .on_click(listener(weak, |this, window, cx| {
                    this.dismiss_menu(window, cx)
                })),
        )
        .children(confirm)
}

/// The main menu's rows, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MainItem {
    Settings,
    Shortcuts,
    Themes,
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    Commands,
    Workspaces,
    ReloadGuiConfig,
    AppUpdates,
    PreviewAppUpdate,
    GitHub,
    About,
    ReloadDaemonConfig,
    UpdateReady,
    Detach,
    Reconnect,
}

impl MainItem {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Settings => "Settings",
            Self::Shortcuts => "Keyboard Shortcuts",
            Self::Themes => "Themes",
            Self::IncreaseFontSize => "Increase Font Size",
            Self::DecreaseFontSize => "Decrease Font Size",
            Self::ResetFontSize => "Reset Font Size",
            Self::Commands => "Commands",
            Self::Workspaces => "Workspaces",
            Self::ReloadGuiConfig => "Reload GUI Config",
            Self::AppUpdates => "App Updates",
            Self::PreviewAppUpdate => "Preview App Update",
            Self::GitHub => "GitHub Sign-in",
            Self::About => "About",
            Self::ReloadDaemonConfig => "Reload Daemon Config",
            Self::UpdateReady => "Update Ready",
            Self::Detach => "Detach",
            Self::Reconnect => "Reconnect",
        }
    }
}

impl HerdrWindow {
    pub(crate) fn show_install_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.begin_menu(window, cx) {
            return;
        }
        self.show_dialog(Page::Install, window, cx, |_, dialog, weak, _, _| {
            dialog
                .title("Herdr must be installed")
                .child(
                    "Install Herdr first, then choose Terminal > Reconnect. The Install button \
                     opens the Herdr website; nothing is installed automatically.",
                )
                .footer(dialog_buttons(
                    weak,
                    Some(
                        Button::new("menu-install")
                            .primary()
                            .label("Install")
                            .on_click(|_, _, cx| cx.open_url(crate::about::WEBSITE)),
                    ),
                ))
                .on_ok(|_, _, cx| {
                    cx.open_url(crate::about::WEBSITE);
                    false
                })
        });
    }

    /// Prepares a new menu page: ends any theme preview, forgets the previous
    /// page, closes its kit surfaces, and fences the page to the current
    /// connection. Returns false when a theme save must finish first.
    pub(crate) fn begin_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.cancel_theme_preview(cx) {
            return false;
        }
        self.menu.reset();
        window.close_all_dialogs(cx);
        self.menu.endpoint_target = (
            self.selection_epoch,
            self.endpoints[self.selected_endpoint].generation,
        );
        self.marked.clear();
        true
    }

    /// Opens the main menu at `menu.anchor`.
    pub(crate) fn open_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.begin_menu(window, cx) {
            return false;
        }
        let weak = cx.weak_entity();
        let items = self.menu_items();
        self.show_popup(
            Page::Menu,
            self.menu.anchor,
            window,
            cx,
            move |menu, _, _| {
                items.into_iter().fold(menu.min_w(px(200.)), |menu, item| {
                    menu.item(
                        PopupMenuItem::new(item.label())
                            .on_click(listener(&weak, move |this, window, cx| {
                                this.activate_menu(item, window, cx)
                            })),
                    )
                })
            },
        );
        true
    }

    /// Whether a menu popup or dialog holds input.
    pub(crate) fn menu_is_open(&self) -> bool {
        self.menu.page.is_some()
    }

    /// Closes whatever menu popup or dialog is open.
    pub(crate) fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_menu(window, cx);
    }

    pub(crate) fn dismiss_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.cancel_theme_preview(cx) {
            return;
        }
        // Whatever the pointer was resting on, this dismissal ends that intent.
        self.hover = None;
        self.hover_menu = None;
        self.update_preview = None;
        self.menu.reset();
        window.close_all_dialogs(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Reconciles the kit layer with `menu.page` after a reset that had no
    /// window to close surfaces with, or a dialog the kit closed on its own.
    pub(crate) fn sync_menu_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog_open = window.has_active_dialog(cx);
        match self.menu.page {
            Some(page) if !page.is_popup() && !dialog_open => self.dismiss_menu(window, cx),
            Some(page) if page.is_popup() && self.menu.popup.is_none() => {
                self.dismiss_menu(window, cx)
            }
            None if dialog_open || self.menu.popup.is_some() => self.dismiss_menu(window, cx),
            _ => {}
        }
    }

    /// Installs the open dialog's text field holding `text`.
    pub(crate) fn set_menu_input(
        &mut self,
        text: impl Into<SharedString>,
        placeholder: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let text = text.into();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(text)
        });
        self.menu.input = Some(input.clone());
        input
    }

    /// Focuses the dialog's field with its text selected, so typing replaces
    /// it. Called after the dialog opened, which focuses the dialog itself.
    pub(crate) fn focus_menu_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = &self.menu.input {
            input.update(cx, |input, cx| {
                input.select_all(window, cx);
                input.focus(window, cx);
            });
        }
    }

    pub(crate) fn menu_target_current(&self) -> bool {
        self.menu.endpoint_target
            == (
                self.selection_epoch,
                self.endpoints[self.selected_endpoint].generation,
            )
    }

    /// Shows `page` as a kit popup menu at `anchor`. Dismissal returns focus to
    /// the terminal and forgets the page, unless an item already moved on.
    pub(crate) fn show_popup(
        &mut self,
        page: Page,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: impl FnOnce(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu,
    ) {
        let focus = self.focus.clone();
        let menu = PopupMenu::build(window, cx, |menu, window, cx| {
            build(menu.action_context(focus), window, cx)
        });
        let dismiss = cx.subscribe_in(
            &menu,
            window,
            |this, menu: &Entity<PopupMenu>, _: &DismissEvent, window, cx| {
                // An item that opened a dialog or another popup has already
                // replaced this one; its dismissal must not close the new page.
                if this
                    .menu
                    .popup
                    .as_ref()
                    .is_some_and(|popup| popup.menu == *menu)
                {
                    this.dismiss_menu(window, cx);
                }
            },
        );
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.menu.page = Some(page);
        self.menu.anchor = anchor;
        self.menu.popup = Some(Popup {
            menu,
            anchor,
            corner: Anchor::TopLeft,
            _dismiss: dismiss,
        });
        cx.notify();
    }

    /// Shows `page` as a kit dialog. `build` runs on every frame from this
    /// window's current state, so the dialog follows daemon answers without a
    /// copy of its own. Cancel, Escape, the close button, and a backdrop click
    /// all dismiss through `dismiss_menu`, which is the only path that closes it.
    pub(crate) fn show_dialog(
        &mut self,
        page: Page,
        window: &mut Window,
        cx: &mut Context<Self>,
        build: impl Fn(&HerdrWindow, Dialog, &WeakEntity<HerdrWindow>, &mut Window, &App) -> Dialog
        + 'static,
    ) {
        self.menu.popup = None;
        self.menu.page = Some(page);
        window.close_all_dialogs(cx);
        let weak = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            let dialog = dialog.on_cancel({
                let weak = weak.clone();
                move |_, window, cx| {
                    let _ = weak.update(cx, |this, cx| this.dismiss_menu(window, cx));
                    false
                }
            });
            let Some(view) = weak.upgrade() else {
                return dialog;
            };
            let this = view.read(cx);
            if this.menu.page != Some(page) {
                return dialog;
            }
            build(this, dialog, &weak, window, cx)
        });
        cx.notify();
    }

    pub(super) fn menu_items(&self) -> Vec<MainItem> {
        let mut items = vec![
            MainItem::Settings,
            MainItem::Shortcuts,
            MainItem::Themes,
            MainItem::IncreaseFontSize,
            MainItem::DecreaseFontSize,
            MainItem::ResetFontSize,
            MainItem::Commands,
            MainItem::Workspaces,
            MainItem::ReloadGuiConfig,
            MainItem::AppUpdates,
            MainItem::PreviewAppUpdate,
            MainItem::GitHub,
            MainItem::About,
        ];
        if self.live.status.is_connected() {
            items.push(MainItem::ReloadDaemonConfig);
        }
        if self
            .live
            .snapshot
            .as_ref()
            .is_some_and(|s| s.update_available.is_some())
        {
            items.push(MainItem::UpdateReady);
        }
        items.push(
            if self.endpoints[self.selected_endpoint]
                .connection
                .handle
                .is_some()
            {
                MainItem::Detach
            } else {
                MainItem::Reconnect
            },
        );
        items
    }

    pub(super) fn activate_menu(
        &mut self,
        item: MainItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match item {
            MainItem::GitHub => self.open_github(false, window, cx),
            MainItem::About => self.open_about(window, cx),
            MainItem::Settings => self.open_preferences(window, cx),
            MainItem::Shortcuts => self.open_keybinds(window, cx),
            MainItem::Themes => self.open_theme_picker(window, cx),
            MainItem::IncreaseFontSize | MainItem::DecreaseFontSize | MainItem::ResetFontSize => {
                use crate::config::FONT_SIZE_STEP;
                let size = match item {
                    MainItem::IncreaseFontSize => self.config.terminal.size + FONT_SIZE_STEP,
                    MainItem::DecreaseFontSize => self.config.terminal.size - FONT_SIZE_STEP,
                    _ => self.configured_terminal_size,
                };
                // `command` refuses to act while a page is open, so apply here.
                self.set_terminal_font_size(size, cx);
                self.dismiss_menu(window, cx);
            }
            MainItem::Commands => self.open_palette(false, window, cx),
            MainItem::Workspaces => self.open_palette(true, window, cx),
            MainItem::UpdateReady => self.open_update_ready(window, cx),
            MainItem::AppUpdates => self.open_app_update(false, window, cx),
            MainItem::PreviewAppUpdate => self.open_app_update(true, window, cx),
            MainItem::ReloadGuiConfig => self.reload_gui_config(window, cx),
            MainItem::ReloadDaemonConfig => {
                if let (Some(handle), Some(snapshot)) = (
                    &self.endpoints[self.selected_endpoint].connection.handle,
                    &self.live.snapshot,
                ) {
                    self.local_error = handle
                        .request(
                            &snapshot.boot_id,
                            Method::ServerReloadConfig,
                            serde_json::json!({}),
                        )
                        .err()
                        .map(|error| format!("Reload config: {error}"));
                }
                self.dismiss_menu(window, cx);
            }
            MainItem::Detach => {
                self.detach_endpoint();
                self.dismiss_menu(window, cx);
            }
            MainItem::Reconnect => {
                self.reconnect();
                self.dismiss_menu(window, cx);
            }
        }
        cx.notify();
    }

    /// What the daemon says about its own pending upgrade. Nothing here runs
    /// the install command; the user reviews and runs it themselves.
    fn open_update_ready(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.begin_menu(window, cx) {
            return;
        }
        self.show_dialog(Page::Update, window, cx, |this, dialog, weak, _, cx| {
            let snapshot = this.live.snapshot.as_ref();
            let version = snapshot
                .and_then(|s| s.update_available.clone())
                .unwrap_or_else(|| "unavailable".into());
            let command = snapshot
                .map(|s| s.update_install_command.clone())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "No install command provided by daemon.".into());
            dialog
                .title("Update ready")
                .child(
                    v_flex()
                        .gap_2()
                        .child(format!("Version: {version}"))
                        .child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .child("Suggested command (review and run yourself):"),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child(div().flex_1().min_w_0().child(command.clone()))
                                .child(
                                    component::clipboard::Clipboard::new("update-command-copy")
                                        .value(command),
                                ),
                        )
                        .child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .child("Nothing is installed or executed by this panel."),
                        ),
                )
                .footer(
                    h_flex().w_full().justify_end().child(
                        Button::new("menu-close")
                            .label("Close")
                            .icon(IconName::Close)
                            .on_click(listener(weak, |this, window, cx| {
                                this.dismiss_menu(window, cx)
                            })),
                    ),
                )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Page;
    use gpui_kit::{TestAppContext, component::WindowExt as _};

    /// The window's overlay layer is where kit dialogs and popups reach the
    /// screen, and `menu.page` stays in step with what the kit shows.
    #[gpui_kit::test]
    fn dialogs_and_popups_render_in_the_overlay_and_dismiss_together(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        let draw = |cx: &mut gpui_kit::VisualTestContext| {
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })
        };
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_about(window, cx)));
        draw(cx);
        assert!(cx.debug_bounds("dialog-layer").is_some());
        assert!(cx.debug_bounds("about").is_some());
        cx.update(|window, cx| {
            assert!(window.has_active_dialog(cx));
            view.update(cx, |view, cx| {
                assert_eq!(view.menu.page, Some(Page::About));
                view.open_menu(window, cx);
                // A new page replaces the dialog rather than stacking on it.
                assert!(!window.has_active_dialog(cx));
                assert_eq!(view.menu.page, Some(Page::Menu));
            })
        });
        draw(cx);
        assert!(cx.debug_bounds("menu-panel").is_some());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.dismiss_menu(window, cx);
                assert!(view.menu.page.is_none());
                assert!(view.focus.is_focused(window));
            })
        });
        draw(cx);
        assert!(cx.debug_bounds("menu-panel").is_none());
    }
}
