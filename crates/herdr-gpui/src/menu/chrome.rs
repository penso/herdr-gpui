//! The popup shell: opening and dismissing it, keeping its input isolated from
//! the panes beneath, and painting the page the menu is currently on. Geometry
//! here is the same geometry used for hit testing and IME placement.

use super::{MENU_MARGIN, Page, WorkspaceAction};
use crate::{HerdrWindow, actions, fonts::StyledFont};
use gpui::{prelude::*, *};
use herdr_client::Method;

impl HerdrWindow {
    pub(crate) fn show_install_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::Install);
    }

    /// An Edit menu item while a menu page holds focus. Only the targets the
    /// overlay's key handler gives these shortcuts to are reached: a dialog's
    /// text draft, and the GitHub page's device code for Copy. Search fields
    /// take the action themselves before it bubbles here. Handlers that act
    /// without stopping propagation are never called, so a shortcut the
    /// overlay leaves unhandled cannot run twice through the menu bar.
    fn menu_edit(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let event = actions::edit_key(key);
        if self.menu.page == Some(Page::Dialog(WorkspaceAction::OpenWorktree))
            || self.worktree_listing()
        {
            return;
        }
        if let Some(input) = self.menu.input.as_mut() {
            if input.key(&event.keystroke, cx) {
                cx.notify();
            }
            return;
        }
        if self.menu.page == Some(Page::GitHub) {
            self.github_key(&event, window, cx);
        }
    }

    pub(crate) fn open_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.cancel_theme_preview(cx) {
            return false;
        }
        self.menu.reset();
        self.menu.endpoint_target = (
            self.selection_epoch,
            self.endpoints[self.selected_endpoint].generation,
        );
        self.menu.page = Some(Page::Menu);
        self.marked.clear();
        window.focus(&self.menu.focus, cx);
        cx.notify();
        true
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
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn restore_menu_focus(&self, window: &mut Window, cx: &mut App) {
        if self.menu.page.is_none() && self.menu.focus.is_focused(window) {
            window.focus(&self.focus, cx);
        }
    }

    pub(crate) fn menu_target_current(&self) -> bool {
        self.menu.endpoint_target
            == (
                self.selection_epoch,
                self.endpoints[self.selected_endpoint].generation,
            )
    }

    pub(super) fn menu_items(&self) -> Vec<&'static str> {
        let mut items = vec![
            "settings",
            "shortcuts",
            "themes",
            "increase font size",
            "decrease font size",
            "reset font size",
            "commands",
            "workspaces",
            "reload GUI config",
            "app updates",
            "preview app update",
            "GitHub sign-in",
            "about",
        ];
        if self.live.status.is_connected() {
            items.push("reload daemon config");
        }
        if self
            .live
            .snapshot
            .as_ref()
            .is_some_and(|s| s.update_available.is_some())
        {
            items.push("update ready");
        }
        items.push(
            if self.endpoints[self.selected_endpoint]
                .connection
                .handle
                .is_some()
            {
                "detach"
            } else {
                "reconnect"
            },
        );
        items
    }

    pub(super) fn activate_menu(
        &mut self,
        item: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match item {
            "GitHub sign-in" => self.menu.page = Some(Page::GitHub),
            "about" => self.open_about(window, cx),
            "settings" => self.open_preferences(window, cx),
            "shortcuts" => self.open_keybinds(window, cx),
            "themes" => self.open_theme_picker(window, cx),
            "increase font size" | "decrease font size" | "reset font size" => {
                use crate::config::FONT_SIZE_STEP;
                let size = match item {
                    "increase font size" => self.config.terminal.size + FONT_SIZE_STEP,
                    "decrease font size" => self.config.terminal.size - FONT_SIZE_STEP,
                    _ => self.configured_terminal_size,
                };
                // `command` refuses to act while a page is open, so apply here.
                self.set_terminal_font_size(size, cx);
                self.dismiss_menu(window, cx);
            }
            "commands" => self.open_palette(false, window, cx),
            "workspaces" => self.open_palette(true, window, cx),
            "update ready" => self.menu.page = Some(Page::Update),
            "app updates" => self.open_app_update(false, window, cx),
            "preview app update" => self.open_app_update(true, window, cx),
            "reload GUI config" => self.reload_gui_config(window, cx),
            "reload daemon config" => {
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
            "detach" => {
                self.detach_endpoint();
                self.dismiss_menu(window, cx);
            }
            "reconnect" => {
                self.reconnect();
                self.dismiss_menu(window, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    pub(crate) fn render_menu(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let page = self.menu.page.unwrap_or(Page::Menu);
        let font = &self.config.ui;
        let theme = &self.theme;
        let viewport = window.viewport_size();
        // A GitHub tab of the new worktree dialog is a picker, not a form.
        let listing =
            self.worktree_listing() || page == Page::Dialog(WorkspaceAction::OpenWorktree);
        // The new worktree dialog keeps a listing's size on every tab, form
        // included, so moving between tabs never resizes it.
        let settled = listing || page == Page::Dialog(WorkspaceAction::NewWorktree);
        // Context menus open where the pointer asked for them. A dialog is a
        // modal decision, not a continuation of the row it came from, so it
        // centres over a dimmed window the way the Herdr TUI's dialogs do.
        let pointer_anchored = matches!(
            page,
            Page::Workspace
                | Page::Tab
                | Page::RenameTab
                | Page::Pane
                | Page::RenamePane
                | Page::Host
                | Page::RemoveDevice
                | Page::Git
                | Page::GitCommit
        );
        let mut panel = div()
            .id("menu-panel")
            .debug_selector(|| "menu-panel".into())
            .when(matches!(page, Page::Workspace | Page::Dialog(_)), |panel| {
                panel
                    .w(px(if page == Page::Workspace {
                        340.
                    } else if page == Page::Dialog(WorkspaceAction::DeleteWorktree) {
                        480.
                    } else if settled {
                        // A listing needs room for a title and its branch.
                        560.
                    } else {
                        420.
                    })
                    .min((viewport.width - px(24.)).max(px(0.))))
                    // Every dialog may use the window's height: a captioned form
                    // whose buttons need scrolling into view reads as clipped.
                    .max_h((viewport.height - px(24.)).max(px(0.)))
                    // A listing is a picker: it takes a settled height and
                    // scrolls inside it, as the theme and command pickers do.
                    .when(settled, |panel| {
                        panel
                            .flex()
                            .flex_col()
                            .h(px(560. * (font.size / 12.))
                                .min((viewport.height - px(24.)).max(px(0.))))
                            .overflow_hidden()
                    })
                    // Lift the popup off the terminal behind it, as the pickers do.
                    .shadow_lg()
            })
            .when(matches!(page, Page::Menu | Page::Devices), |panel| {
                // Open on whichever side of the anchor has room, and keep a
                // margin from the window chrome and the bottom edge: a clamped
                // list then reads as scrollable rather than clipped.
                let chrome = px(crate::titlebar::HEIGHT
                    + crate::worktree_banner::reserved(env!("HERDR_BUILD_WORKTREE") == "1"));
                let band = (viewport.height - chrome - px(2. * MENU_MARGIN)).max(px(60.));
                let room = |side: Pixels| side.clamp(px(0.), band).max(px(60.)).min(band);
                let above = room(self.menu.anchor.y - px(12. + MENU_MARGIN) - chrome);
                let below = room(viewport.height - self.menu.anchor.y - px(12. + MENU_MARGIN));
                let panel = panel
                    .absolute()
                    .left(if page == Page::Devices {
                        self.menu.anchor.x
                    } else {
                        px(56.)
                    })
                    .w(px(if page == Page::Devices {
                        super::devices::MENU_WIDTH
                    } else {
                        180.
                    })
                    .min((viewport.width - px(16.)).max(px(0.))));
                if above >= below {
                    panel
                        .bottom(
                            (viewport.height - self.menu.anchor.y
                                + px(if page == Page::Devices {
                                    super::devices::MENU_GAP
                                } else {
                                    12.
                                }))
                            .max(px(MENU_MARGIN)),
                        )
                        .max_h(above)
                } else {
                    panel.top(self.menu.anchor.y + px(12.)).max_h(below)
                }
            })
            .when(matches!(page, Page::Usage(_)), |panel| {
                // Rises from the status bar segment that opened it, kept inside
                // the window and clear of the titlebar.
                let chrome = px(crate::titlebar::HEIGHT
                    + crate::worktree_banner::reserved(env!("HERDR_BUILD_WORKTREE") == "1"));
                let width = px(crate::usage::PANEL_WIDTH)
                    .min((viewport.width - px(2. * MENU_MARGIN)).max(px(0.)));
                panel
                    .absolute()
                    .left(
                        self.menu
                            .anchor
                            .x
                            .min(viewport.width - width - px(MENU_MARGIN))
                            .max(px(MENU_MARGIN)),
                    )
                    .bottom((viewport.height - self.menu.anchor.y).max(px(MENU_MARGIN)))
                    .w(width)
                    .max_h((self.menu.anchor.y - chrome - px(MENU_MARGIN)).max(px(60.)))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .shadow_lg()
            })
            .when(matches!(page, Page::Git | Page::GitCommit), |panel| {
                panel
                    .w((viewport.width - px(24.))
                        .max(px(0.))
                        .min(px(if page == Page::Git { 340. } else { 420. })))
                    .max_h((viewport.height - px(24.)).max(px(0.)))
                    .when(page == Page::Git, |panel| {
                        let top = crate::titlebar::HEIGHT
                            + crate::worktree_banner::reserved(env!("HERDR_BUILD_WORKTREE") == "1")
                            + 6.;
                        panel
                            .max_h((viewport.height - px(top + 12.)).max(px(0.)))
                            .shadow_lg()
                    })
            })
            .when(
                matches!(
                    page,
                    Page::Tab
                        | Page::RenameTab
                        | Page::Pane
                        | Page::RenamePane
                        | Page::Host
                        | Page::RemoveDevice
                ),
                |panel| {
                    panel
                        .w((viewport.width - px(24.)).max(px(0.)).min(px(
                            if matches!(page, Page::Tab | Page::Pane | Page::Host) {
                                180.
                            } else {
                                360.
                            },
                        )))
                        .max_h((viewport.height - px(24.)).max(px(0.)))
                },
            )
            .when(
                !matches!(page, Page::Menu | Page::Devices | Page::Usage(_))
                    && !pointer_anchored
                    && !matches!(page, Page::Dialog(_)),
                |panel| {
                    panel
                        .w((viewport.width - px(32.)).max(px(0.)).min(px(480.)))
                        .max_h((viewport.height - px(32.)).max(px(0.)))
                },
            )
            .when(
                !matches!(
                    page,
                    Page::Keybinds
                        | Page::Themes
                        | Page::Palette
                        | Page::Preferences
                        | Page::AppUpdate
                        | Page::GitHub
                        | Page::AddDevice
                        | Page::Usage(_)
                        | Page::RenameDevice
                ),
                |panel| {
                    // Dialogs draw their own full-bleed header and footer rules,
                    // so the panel's own inset would cut those rules short.
                    panel
                        .when(!settled, |panel| panel.overflow_y_scroll())
                        .when(!matches!(page, Page::Dialog(_)), |panel| panel.p(px(6.)))
                },
            )
            .when(
                matches!(
                    page,
                    Page::Keybinds | Page::Themes | Page::Palette | Page::Preferences
                ),
                |panel| {
                    panel
                        .flex()
                        .flex_col()
                        .h(px(560. * (font.size / 12.))
                            .min((viewport.height - px(32.)).max(px(0.))))
                        .overflow_hidden()
                        .shadow_lg()
                },
            )
            .when(page == Page::GitHub, |panel| {
                panel
                    .w((viewport.width - px(32.)).max(px(0.)).min(px(400.)))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .shadow_lg()
            })
            .when(page == Page::Install, |panel| {
                panel
                    .w((viewport.width - px(24.)).max(px(0.)).min(px(420.)))
                    .max_h((viewport.height - px(24.)).max(px(0.)))
            })
            .when(
                matches!(page, Page::AppUpdate | Page::AddDevice | Page::RenameDevice),
                |panel| panel.flex().flex_col().overflow_hidden().shadow_lg(),
            )
            .when(page == Page::About, |panel| {
                panel.w((viewport.width - px(24.)).max(px(0.)).min(px(340.)))
            })
            .rounded(px(crate::config::corners::PANEL))
            .border_1()
            .border_color(rgb(theme.active))
            // Popup pages (preferences, pickers, dialogs, about and update)
            // must remain legible even when the terminal window is transparent.
            .bg(rgb(theme.surface))
            .text_color(rgb(theme.foreground))
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(font.line_height()))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            // A menu the pointer opened follows the pointer's own report of
            // whether it is over the popup, which occlusion and snapping make
            // impossible to infer from the anchor alone.
            .when(self.hover_menu.is_some(), |panel| {
                panel.on_hover(cx.listener(|this, hovered: &bool, _, _| {
                    if let Some(open) = &mut this.hover_menu {
                        open.inside = *hovered;
                    }
                }))
            });
        if page == Page::Menu {
            for (index, item) in self.menu_items().into_iter().enumerate() {
                panel = panel.child(
                    div()
                        .id(item)
                        .debug_selector(move || format!("menu-{item}"))
                        .min_h(px(font.line_height() + 12.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .when(Some(index) == self.menu.selected, |row| {
                            row.bg(rgb(theme.active))
                        })
                        .on_hover(cx.listener(move |this, hovered, _, cx| {
                            if *hovered {
                                this.menu.selected = Some(index);
                            } else if this.menu.selected == Some(index) {
                                this.menu.selected = None;
                            }
                            cx.notify();
                        }))
                        .child(item)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.activate_menu(item, window, cx);
                        })),
                );
            }
        } else if page == Page::Devices {
            panel = panel.child(self.render_devices(cx));
        } else if let Page::Usage(provider) = page {
            panel = panel.child(self.render_usage_panel(provider, cx));
        } else if page == Page::AddDevice {
            panel = panel.child(self.render_add_device(cx));
        } else if page == Page::GitHub {
            panel = panel.child(self.render_github_auth(cx));
        } else if page == Page::Workspace {
            if let Some(target) = &self.menu.target {
                panel = panel.child(
                    div()
                        .debug_selector(|| "workspace-menu-header".into())
                        .px(px(8.))
                        .py(px(6.))
                        .mb(px(4.))
                        .border_b_1()
                        .border_color(rgb(theme.active))
                        .child(
                            div()
                                .debug_selector(|| "workspace-menu-name".into())
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(crate::sidebar::label_text(&target.label)),
                        )
                        .when_some(
                            target
                                .branch
                                .as_deref()
                                .filter(|branch| !branch.trim().is_empty()),
                            |header, branch| {
                                header.child(
                                    div()
                                        .debug_selector(|| "workspace-menu-branch".into())
                                        .truncate()
                                        .text_color(rgb(theme.muted))
                                        .text_size(px(font.size * 0.9))
                                        .child(crate::sidebar::label_text(branch)),
                                )
                            },
                        ),
                );
            }
            for (action, label) in self.workspace_items() {
                panel = panel.child(
                    div()
                        .id(label)
                        .debug_selector(move || format!("workspace-menu-{label}"))
                        .min_h(px(font.line_height() + 12.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .cursor_pointer()
                        .rounded(px(crate::config::corners::CONTROL))
                        .when(Some(action) == self.menu.workspace_selected, |row| {
                            row.bg(rgb(theme.active))
                        })
                        .on_hover(cx.listener(move |this, hovered, _, cx| {
                            if *hovered {
                                this.menu.workspace_selected = Some(action);
                            } else if this.menu.workspace_selected == Some(action) {
                                this.menu.workspace_selected = None;
                            }
                            cx.notify();
                        }))
                        .when_some(action.icon(), |row, icon| {
                            row.child(
                                svg()
                                    .path(icon)
                                    .debug_selector(move || format!("workspace-menu-icon-{label}"))
                                    .size(px(14.))
                                    .flex_none()
                                    .text_color(rgb(theme.muted)),
                            )
                        })
                        .child(label)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.activate_workspace_menu(action, window, cx);
                        })),
                );
            }
            if self.pr_profile().is_some() {
                panel = panel.child(self.render_workspace_pr(
                    (px(340.).min((viewport.width - px(24.)).max(px(0.))) - px(30.)).max(px(0.)),
                    cx,
                ));
            }
        } else if let Page::Dialog(action) = page {
            panel = panel.child(self.render_workspace_dialog(action, cx));
        } else if page == Page::Git {
            panel = panel.child(self.render_git_menu(cx));
        } else if page == Page::GitCommit {
            panel = panel.child(self.render_git_commit(cx));
        } else if matches!(page, Page::Host | Page::RenameDevice | Page::RemoveDevice) {
            panel = panel.child(self.render_host_menu(cx));
        } else if matches!(page, Page::Tab | Page::RenameTab) {
            panel = panel.child(self.render_tab_menu(cx));
        } else if matches!(page, Page::Pane | Page::RenamePane) {
            panel = panel.child(self.render_pane_menu(cx));
        } else if page == Page::Keybinds {
            panel = panel.child(self.render_keybinds(cx));
        } else if page == Page::Themes {
            panel = panel.child(self.render_theme_picker(cx));
        } else if page == Page::Palette {
            panel = panel.child(self.render_palette(cx));
        } else if page == Page::ConfirmClose {
            panel = panel.child(self.render_close_confirmation(cx));
        } else if page == Page::Preferences {
            panel = panel.child(self.render_preferences(cx));
        } else if page == Page::AppUpdate {
            panel = panel.child(self.render_app_update(window, cx));
        } else if page == Page::About {
            panel = panel.child(self.render_about(cx));
        } else if page == Page::Install {
            panel = panel
                .child(div().p(px(8.)).child("Herdr must be installed"))
                .child(div().p(px(8.)).child(
                    "Install Herdr first, then choose Terminal > Reconnect. The Install button opens the Herdr website; nothing is installed automatically.",
                ))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .p(px(8.))
                        .child(
                            div()
                                .id("menu-install")
                                .debug_selector(|| "menu-install".into())
                                .p(px(8.))
                                .rounded(px(crate::config::corners::CONTROL))
                                .bg(rgb(theme.active))
                                .cursor_pointer()
                                .child("Install")
                                .on_click(|_, _, cx| {
                                    cx.stop_propagation();
                                    cx.open_url(crate::about::WEBSITE);
                                }),
                        )
                        .child(
                            div()
                                .id("menu-dismiss")
                                .debug_selector(|| "menu-dismiss".into())
                                .p(px(8.))
                                .rounded(px(crate::config::corners::CONTROL))
                                .hover(|button| button.bg(rgb(theme.active)))
                                .cursor_pointer()
                                .child("Dismiss")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.dismiss_menu(window, cx);
                                })),
                        ),
                );
        } else {
            let (title, rows) = {
                let snapshot = self.live.snapshot.as_ref();
                (
                    "Update ready",
                    vec![
                        format!(
                            "Version: {}",
                            snapshot
                                .and_then(|s| s.update_available.as_deref())
                                .unwrap_or("unavailable")
                        ),
                        "Suggested command (review and run yourself):".into(),
                        snapshot
                            .map(|s| s.update_install_command.clone())
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or("No install command provided by daemon.".into()),
                        "Nothing is installed or executed by this panel.".into(),
                    ],
                )
            };
            panel = panel.child(div().p(px(8.)).child(title));
            for text in rows {
                panel = panel.child(div().p(px(8.)).child(text));
            }
            panel = panel.child(
                div()
                    .id("menu-close")
                    .p(px(8.))
                    .cursor_pointer()
                    .child("Close")
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.dismiss_menu(window, cx);
                    })),
            );
        }
        div()
            .id("menu-overlay")
            .absolute()
            .inset_0()
            .when(
                !matches!(page, Page::Menu | Page::Devices | Page::Usage(_)) && !pointer_anchored,
                |overlay| {
                    overlay
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgba((theme.background << 8) | 0xb0))
                },
            )
            .occlude()
            .track_focus(&self.menu.focus)
            .on_action(
                cx.listener(|this, _: &actions::Cut, window, cx| this.menu_edit("x", window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &actions::Copy, window, cx| this.menu_edit("c", window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &actions::Paste, window, cx| this.menu_edit("v", window, cx)),
            )
            .on_action(cx.listener(|this, _: &actions::SelectAll, window, cx| {
                this.menu_edit("a", window, cx)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.dismiss_menu(window, cx);
                }),
            )
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, window, cx| {
                    if this.menu.opening_right_click {
                        cx.stop_propagation();
                        return;
                    }
                    // Only workspace rows may retarget this gesture. The overlay
                    // still occludes ordinary terminal and chrome handlers.
                    if this.menu.page != Some(Page::Workspace) {
                        cx.stop_propagation();
                    }
                    this.dismiss_menu(window, cx);
                }),
            )
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.menu.page == Some(Page::Dialog(WorkspaceAction::OpenWorktree))
                    && (this
                        .menu
                        .worktree_open
                        .as_ref()
                        .is_some_and(|picker| picker.search.read(cx).is_composing())
                        || !matches!(
                            event.keystroke.key.as_str(),
                            "escape" | "enter" | "up" | "down"
                        ))
                {
                    // SearchInput and the platform own text editing and composition.
                    return;
                }
                // A listing has its own search field, so the branch draft must
                // not consume the keys typed into it.
                if this.worktree_source_key(event, window, cx) {
                    cx.stop_propagation();
                    window.prevent_default();
                    return;
                }
                let listing = this.worktree_listing() || this.worktree_search_focused(window, cx);
                // The shared search owns its own typing, so everything the list
                // itself does not own must reach the native text handler.
                if listing
                    && (this.worktree_source_composing(cx)
                        || event.keystroke.key.as_str() != "escape")
                {
                    return;
                }
                // The name field edits itself; only Escape and Enter are left
                // for the dialog, and neither may reach the branch draft.
                let naming = this.worktree_name_focused(window, cx);
                if naming
                    && (this
                        .menu
                        .worktree
                        .as_ref()
                        .is_some_and(|source| source.name.read(cx).is_composing())
                        || !matches!(event.keystroke.key.as_str(), "escape" | "enter"))
                {
                    return;
                }
                if let Some(input) = this.menu.input.as_mut().filter(|_| !listing && !naming) {
                    if input.key(&event.keystroke, cx) {
                        cx.stop_propagation();
                        window.prevent_default();
                        cx.notify();
                        return;
                    }
                    // Let the platform deliver printable text and IME navigation/commit.
                    if input.marked.is_some()
                        || !matches!(event.keystroke.key.as_str(), "escape" | "enter")
                    {
                        return;
                    }
                }
                if this.menu.page == Some(Page::Git) {
                    this.git_key(event, window, cx);
                    return;
                }
                if matches!(
                    this.menu.page,
                    Some(Page::Host | Page::RenameDevice | Page::RemoveDevice)
                ) {
                    this.host_menu_key(event, window, cx);
                    return;
                }
                if matches!(this.menu.page, Some(Page::Tab | Page::RenameTab)) {
                    this.tab_menu_key(event, window, cx);
                    return;
                }
                if matches!(this.menu.page, Some(Page::Pane | Page::RenamePane)) {
                    this.pane_menu_key(event, window, cx);
                    return;
                }
                if let Some(Page::Usage(provider)) = this.menu.page
                    && this.usage_key(provider, event, cx)
                {
                    cx.stop_propagation();
                    window.prevent_default();
                    return;
                }
                if matches!(this.menu.page, Some(Page::Devices | Page::AddDevice)) {
                    this.devices_key(event, window, cx);
                    return;
                }
                if this.menu.page == Some(Page::Palette) {
                    this.palette_key(event, window, cx);
                    return;
                }
                if this.menu.page == Some(Page::ConfirmClose) {
                    this.close_confirmation_key(event, window, cx);
                    return;
                }
                if this.menu.page == Some(Page::GitHub) {
                    this.github_key(event, window, cx);
                    return;
                }
                if this.menu.page == Some(Page::Themes) {
                    this.theme_picker_key(event, window, cx);
                    return;
                }
                if this.menu.page == Some(Page::Keybinds)
                    && (this
                        .menu
                        .keybinds_search
                        .as_ref()
                        .is_some_and(|search| search.read(cx).is_composing())
                        || !matches!(
                            event.keystroke.key.as_str(),
                            "escape" | "up" | "down" | "pageup" | "pagedown"
                        ))
                {
                    // Printable input and IME commands must reach the native text handler.
                    return;
                }
                cx.stop_propagation();
                window.prevent_default();
                match event.keystroke.key.as_str() {
                    "escape" => this.dismiss_menu(window, cx),
                    "up" | "down"
                        if this.menu.page == Some(Page::Dialog(WorkspaceAction::OpenWorktree)) =>
                    {
                        if let Some(picker) = &mut this.menu.worktree_open
                            && !picker.filtered.is_empty()
                            && this.menu.creation.is_none()
                        {
                            let count = picker.filtered.len();
                            picker.selected = if event.keystroke.key == "up" {
                                (picker.selected + count - 1) % count
                            } else {
                                (picker.selected + 1) % count
                            };
                            picker
                                .scroll
                                .scroll_to_item(picker.selected, ScrollStrategy::Top);
                            cx.notify();
                        }
                    }
                    "enter" if matches!(this.menu.page, Some(Page::Dialog(_))) => {
                        this.submit_workspace_dialog(window, cx)
                    }
                    "enter" if this.menu.page == Some(Page::GitCommit) => {
                        this.submit_git_commit(cx)
                    }
                    "up" | "down" if this.menu.page == Some(Page::Workspace) => {
                        let actions = this.workspace_menu_actions();
                        let selected = this.menu.workspace_selected.and_then(|selected| {
                            actions.iter().position(|action| *action == selected)
                        });
                        if !actions.is_empty() {
                            let index = match (selected, event.keystroke.key.as_str()) {
                                (None, "up") => actions.len() - 1,
                                (None, _) => 0,
                                (Some(index), "up") => (index + actions.len() - 1) % actions.len(),
                                (Some(index), _) => (index + 1) % actions.len(),
                            };
                            this.menu.workspace_selected = Some(actions[index]);
                        }
                        cx.notify();
                    }
                    "up" | "down" if this.menu.page == Some(Page::Menu) => {
                        let count = this.menu_items().len();
                        if count > 0 {
                            this.menu.selected =
                                Some(match (this.menu.selected, event.keystroke.key.as_str()) {
                                    (None, "up") => count - 1,
                                    (None, _) => 0,
                                    (Some(index), "up") => (index + count - 1) % count,
                                    (Some(index), _) => (index + 1) % count,
                                });
                        }
                        cx.notify();
                    }
                    "enter" if this.menu.page == Some(Page::Workspace) => {
                        if let Some(action) = this
                            .menu
                            .workspace_selected
                            .filter(|action| this.workspace_menu_actions().contains(action))
                        {
                            this.activate_workspace_menu(action, window, cx);
                        }
                    }
                    "up" | "down" | "pageup" | "pagedown"
                        if matches!(this.menu.page, Some(Page::Keybinds | Page::Preferences)) =>
                    {
                        let scroll = if this.menu.page == Some(Page::Preferences) {
                            &this.menu.preferences_scroll
                        } else {
                            &this.menu.keybinds_scroll
                        };
                        let key = event.keystroke.key.as_str();
                        let distance = if key.starts_with("page") {
                            scroll.bounds().size.height * 0.8
                        } else {
                            px(this.config.ui.line_height() * 3.)
                        };
                        let direction = if key.ends_with("up") { 1. } else { -1. };
                        scroll.set_offset(scroll.offset() + point(px(0.), distance * direction));
                        cx.notify();
                    }
                    "enter" if this.menu.page == Some(Page::Install) => {
                        cx.open_url(crate::about::WEBSITE);
                    }
                    "enter" if this.menu.page == Some(Page::About) => {
                        this.dismiss_menu(window, cx);
                    }
                    "enter" if this.menu.page == Some(Page::Menu) => {
                        if let Some(item) = this
                            .menu
                            .selected
                            .and_then(|index| this.menu_items().get(index).copied())
                        {
                            this.activate_menu(item, window, cx);
                        }
                    }
                    _ => {}
                }
            }))
            .child(if pointer_anchored {
                anchored()
                    .position(if page == Page::Git {
                        point(
                            self.menu.anchor.x,
                            px(crate::titlebar::HEIGHT
                                + crate::worktree_banner::reserved(
                                    env!("HERDR_BUILD_WORKTREE") == "1",
                                )
                                + 6.),
                        )
                    } else {
                        self.menu.anchor
                    })
                    .snap_to_window_with_margin(Edges::all(px(12.)))
                    .child(panel)
                    .into_any_element()
            } else {
                panel.into_any_element()
            })
    }
}
