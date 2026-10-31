//! The popup shell: opening and dismissing it, keeping its input isolated from
//! the panes beneath, and painting the page the menu is currently on. Geometry
//! here is the same geometry used for hit testing and IME placement.

// i18n: migrated
mod keys;

use super::{MENU_MARGIN, Page, WorkspaceAction, devices::rail};
use crate::direction::Logical;
use crate::i18n::fl;
use crate::{HerdrWindow, actions, fonts::StyledFont};
use gpui::{prelude::*, *};

/// How far past its panel a popover counts as covering, for the Windows
/// pages that hide under it.
#[cfg(any(windows, test, feature = "integration-test"))]
const COVER_MARGIN: f32 = 8.;

impl HerdrWindow {
    pub(crate) fn show_install_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::Install);
    }

    pub(crate) fn open_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.shift_taps.cancel();
        if !self.cancel_theme_preview(cx) {
            return false;
        }
        self.menu.reset();
        self.apply_shared_theme(cx);
        self.menu.endpoint_target = (
            self.selection_epoch,
            self.endpoints[self.selected_endpoint].generation,
        );
        self.menu.page = Some(Page::Menu);
        self.discard_composition(cx);
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
        self.dismiss_release_notes();
        // Closing the offer without an answer is a "not now": it asks once.
        if self.menu.page == Some(Page::AgentSkill)
            && crate::agent_skill::AgentSkill::choice(cx).is_none()
        {
            crate::agent_skill::AgentSkill::choose(crate::agent_skill::Choice::Declined, cx);
        }
        let return_focus = self
            .menu
            .application
            .as_ref()
            .and_then(|menu| menu.return_focus.clone());
        // A page that animates out keeps what it reads from the reset.
        if let Some(page) = self.menu.page.filter(|page| super::appear::leaves(*page)) {
            let kept = super::appear::Kept::take(&mut self.menu);
            self.menu.appear.keep(page, kept);
        }
        self.menu.reset();
        self.apply_shared_theme(cx);
        window.focus(return_focus.as_ref().unwrap_or(&self.focus), cx);
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

    /// Asks the selected daemon to reread its config file.
    pub(crate) fn reload_daemon_config(&mut self) {
        self.local_error = self.endpoints[self.selected_endpoint]
            .reload_config()
            .err()
            .map(|error| {
                fl!(
                    "menu-chrome-reload-config-failed",
                    error = error.to_string()
                )
            });
    }

    pub(super) fn menu_items(&self) -> Vec<&'static str> {
        let mut items = vec![
            "settings",
            "shortcuts",
            "themes",
            "increase font size", // i18n: data
            "decrease font size", // i18n: data
            "reset font size",    // i18n: data
            "command palette",    // i18n: data
            "workspaces",
            "reload GUI config",  // i18n: data
            "app updates",        // i18n: data
            "preview app update", // i18n: data
            "GitHub sign-in",     // i18n: data
            "about",
        ];
        if self.live.status.is_connected() {
            items.push("reload daemon config"); // i18n: data
        }
        items.extend(self.release_notes_item());
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
            "GitHub sign-in" => self.menu.page = Some(Page::GitHub), // i18n: data
            "about" => self.open_about(window, cx),
            "settings" => self.open_preferences(window, cx),
            "shortcuts" => self.open_keybinds(window, cx),
            "themes" => self.open_theme_picker(window, cx),
            // i18n: data
            "increase font size" | "decrease font size" | "reset font size" => {
                use crate::config::FONT_SIZE_STEP;
                let size = match item {
                    "increase font size" => self.config.terminal.size + FONT_SIZE_STEP, // i18n: data
                    "decrease font size" => self.config.terminal.size - FONT_SIZE_STEP, // i18n: data
                    _ => self.configured_terminal_size,
                };
                // `command` refuses to act while a page is open, so apply here.
                self.set_terminal_font_size(size, cx);
                self.dismiss_menu(window, cx);
            }
            "command palette" => self.open_palette(crate::palette::Filter::All, window, cx), // i18n: data
            "workspaces" => self.open_palette(crate::palette::Filter::Navigation, window, cx),
            "update ready" | "what's new" => self.menu.page = Some(Page::Update), // i18n: data
            "app updates" => self.open_app_update(false, window, cx),             // i18n: data
            "preview app update" => self.open_app_update(true, window, cx),       // i18n: data
            "reload GUI config" => self.reload_gui_config(window, cx),            // i18n: data
            // i18n: data
            "reload daemon config" => {
                self.reload_daemon_config();
                self.dismiss_menu(window, cx);
            }
            "detach" => {
                self.detach_endpoint(cx);
                self.dismiss_menu(window, cx);
            }
            "reconnect" => {
                self.reconnect(cx);
                self.dismiss_menu(window, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    pub(crate) fn render_menu(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.menu.page == Some(Page::Application) {
            return self.render_application_menu(window, cx);
        }
        if self.menu.page == Some(Page::Sessions) && self.menu.session_edit.is_some() {
            let theme = &self.theme;
            let font = &self.config.ui;
            let picker = self
                .anchor_footer_panel(
                    div()
                        .id("session-picker-underlay")
                        .debug_selector(|| "session-picker-underlay".into()),
                    window.viewport_size(),
                    Page::Sessions,
                    px(0.),
                )
                .overflow_y_scroll()
                .p(px(6.))
                .rounded(px(crate::config::corners::PANEL))
                .border_1()
                .border_color(rgb(theme.active))
                .bg(rgb(theme.surface))
                .text_color(rgb(theme.foreground))
                .text_font(font)
                .text_size(px(font.size))
                .line_height(px(font.line_height()))
                .child(self.render_session_list(cx));
            // The picker remains visible in its original position beneath the
            // modal's dimmed, input-occluding layer. Only that top layer owns focus.
            return div()
                .id("session-menu-stack")
                .absolute()
                .inset_0()
                .child(picker)
                .child(self.render_menu_layer(
                    self.menu.page.unwrap_or(Page::Menu),
                    None,
                    window,
                    cx,
                ));
        }
        self.render_menu_layer(self.menu.page.unwrap_or(Page::Menu), None, window, cx)
    }

    /// Places a footer menu beside its anchor, `lift` short of it while it
    /// slides into place.
    fn anchor_footer_panel(
        &self,
        panel: Stateful<Div>,
        viewport: Size<Pixels>,
        page: Page,
        lift: Pixels,
    ) -> Stateful<Div> {
        let (room, above) = self.footer_room(viewport);
        let list = matches!(page, Page::Devices | Page::Sessions);
        let width = if list {
            super::devices::MENU_WIDTH
        } else {
            180.
        };
        // From the window's start edge, which right to left is its right.
        let anchor = match crate::direction::is_rtl() {
            false => self.menu.anchor.x,
            true => viewport.width - self.menu.anchor.x,
        };
        let start = match page {
            Page::Devices => anchor,
            Page::Sessions => anchor.min((viewport.width - px(width + MENU_MARGIN)).max(px(0.))),
            _ => px(56.),
        };
        let panel = panel
            .absolute()
            .inset_s(start)
            .w(px(width).min((viewport.width - px(16.)).max(px(0.))));
        if above {
            panel
                .bottom(
                    (viewport.height - self.menu.anchor.y
                        + px(if list { super::devices::MENU_GAP } else { 12. }))
                    .max(px(MENU_MARGIN))
                        - lift,
                )
                .max_h(room)
        } else {
            panel.top(self.menu.anchor.y + px(12.) - lift).max_h(room)
        }
    }

    /// The room a footer menu has, and whether it opens above its anchor,
    /// as it does from the sidebar footer at the window's foot.
    fn footer_room(&self, viewport: Size<Pixels>) -> (Pixels, bool) {
        let chrome = px(crate::titlebar::HEIGHT);
        let band = (viewport.height - chrome - px(2. * MENU_MARGIN)).max(px(60.));
        let room = |side: Pixels| side.clamp(px(0.), band).max(px(60.)).min(band);
        let above = room(self.menu.anchor.y - px(12. + MENU_MARGIN) - chrome);
        let below = room(viewport.height - self.menu.anchor.y - px(12. + MENU_MARGIN));
        if above >= below {
            (above, true)
        } else {
            (below, false)
        }
    }

    /// Whether the open menu is a dialog that dims the window behind it,
    /// rather than a popover beside what opened it.
    pub(crate) fn menu_dims(&self) -> bool {
        let Some(page) = self.menu.page else {
            return false;
        };
        let session_modal = page == Page::Sessions && self.menu.session_edit.is_some();
        let footer_anchored =
            matches!(page, Page::Menu | Page::Devices | Page::Sessions) && !session_modal;
        !footer_anchored
            && !matches!(page, Page::Usage(_) | Page::Application)
            && !page.pointer_anchored()
    }

    /// Draws `page`: the open menu, or with `leaving` the one that just
    /// closed, still showing that much of itself as it goes.
    pub(super) fn render_menu_layer(
        &self,
        page: Page,
        leaving: Option<f32>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let font = &self.config.ui;
        let theme = &self.theme;
        let viewport = window.viewport_size();
        let session_modal = page == Page::Sessions && self.menu.session_edit.is_some();
        // Add Device's sources share one wide dialog; a narrow window keeps
        // the rail to icons so the form still has room.
        let add_device = rail::Source::of(page).is_some();
        let compact_rail = viewport.width - px(32.) < px(rail::COMPACT_BELOW * (font.size / 12.));
        let footer_anchored =
            matches!(page, Page::Menu | Page::Devices | Page::Sessions) && !session_modal;
        // A GitHub tab of the new worktree dialog is a picker, not a form.
        let listing =
            self.worktree_listing() || page == Page::Dialog(WorkspaceAction::OpenWorktree);
        // The new worktree dialog keeps a listing's size on every tab, form
        // included, so moving between tabs never resizes it.
        let settled = listing || page == Page::Dialog(WorkspaceAction::NewWorktree);
        // Context menus open where the pointer asked for them. A dialog is a
        // modal decision, not a continuation of the row it came from, so it
        // centres over a dimmed window the way the Herdr TUI's dialogs do.
        let pointer_anchored = page.pointer_anchored();
        let dims = self.menu_dims();
        let now = crate::motion::now();
        let shown = leaving.unwrap_or_else(|| self.menu.appear.panel(now));
        let lift = super::appear::Appear::offset(dims, shown);
        let rises = page == Page::Devices;
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
            // All Devices rises out of the footer that opened it, so a box in
            // its place clips it on the way; see the end of this function.
            .when(footer_anchored && !rises, |panel| {
                self.anchor_footer_panel(panel, viewport, page, lift)
            })
            .when(rises, |panel| {
                let (room, above) = self.footer_room(viewport);
                let below = 1. - shown;
                // Its own height, as the last frame laid it out; the box it
                // rises in takes its height from it, so a share of that box
                // would be a share of nothing. Until a frame has measured it,
                // the room it may take keeps it out of sight.
                let height = self.menu.appear.rise_height.get().unwrap_or(room);
                let offset = height * below;
                let measured = self.menu.appear.rise_height.clone();
                panel
                    .w_full()
                    .max_h(room)
                    .top(if above { offset } else { -offset })
                    .child(
                        canvas(
                            move |bounds, _, _| measured.set(Some(bounds.size.height)),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
            })
            .when(matches!(page, Page::Usage(_)), |panel| {
                // Rises from the status bar segment that opened it, kept inside
                // the window and clear of the titlebar.
                let chrome = px(crate::titlebar::HEIGHT);
                let width = px(crate::usage::PANEL_WIDTH)
                    .min((viewport.width - px(2. * MENU_MARGIN)).max(px(0.)));
                // The segment's own x, whichever side the bar starts on.
                panel
                    .absolute()
                    .left(
                        self.menu
                            .anchor
                            .x
                            .min(viewport.width - width - px(MENU_MARGIN))
                            .max(px(MENU_MARGIN)),
                    )
                    .bottom((viewport.height - self.menu.anchor.y).max(px(MENU_MARGIN)) - lift)
                    .w(width)
                    .max_h((self.menu.anchor.y - chrome - px(MENU_MARGIN)).max(px(60.)))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .shadow_lg()
            })
            .when(
                matches!(
                    page,
                    Page::Git | Page::GitCommit | Page::PrReview | Page::PrComment | Page::PrMerge
                ),
                |panel| {
                    panel
                        .w((viewport.width - px(24.))
                            .max(px(0.))
                            .min(px(if page == Page::Git { 340. } else { 420. })))
                        .max_h((viewport.height - px(24.)).max(px(0.)))
                        .when(page == Page::Git, |panel| {
                            let top = crate::titlebar::HEIGHT + 6.;
                            panel
                                .max_h((viewport.height - px(top + 12.)).max(px(0.)))
                                .shadow_lg()
                        })
                        // Its lists scroll inside; the panel never outgrows the window.
                        .when(page == Page::PrReview, |panel| panel.overflow_hidden())
                },
            )
            .when(
                matches!(
                    page,
                    Page::Tab
                        | Page::RenameTab
                        | Page::MoveTab
                        | Page::Group
                        | Page::Pane
                        | Page::RenamePane
                        | Page::MovePane
                        | Page::CarryPane
                        | Page::PaneProcesses
                        | Page::KillProcesses
                        | Page::Host
                        | Page::RemoveDevice
                        | Page::RemoveWsl
                ),
                |panel| {
                    let room = (viewport.width - px(24.)).max(px(0.));
                    // A list of actions takes its longest label's width, which
                    // the UI font and size decide, so no fixed width fits all.
                    let forwards = page == Page::Host && self.host_menu_lists_forwards();
                    if matches!(page, Page::Tab | Page::Pane | Page::Group | Page::Host)
                        && !forwards
                    {
                        let least = if page == Page::Group { 240. } else { 180. };
                        return panel
                            .min_w(px(least).min(room))
                            .max_w(room)
                            .max_h((viewport.height - px(24.)).max(px(0.)));
                    }
                    panel
                        .w(room.min(px(if forwards {
                            // Room for a forward's port, state, and actions.
                            260.
                        } else if page == Page::PaneProcesses {
                            // Name, command, pid, CPU and memory columns.
                            560.
                        } else if matches!(page, Page::MovePane | Page::MoveTab) {
                            240.
                        } else {
                            360.
                        })))
                        .max_h((viewport.height - px(24.)).max(px(0.)))
                },
            )
            .when(
                !footer_anchored
                    && !matches!(page, Page::Usage(_))
                    && !pointer_anchored
                    && !matches!(page, Page::Dialog(_)),
                |panel| {
                    panel
                        .w((viewport.width - px(32.))
                            .max(px(0.))
                            .min(px(if add_device {
                                rail::WIDTH * (font.size / 12.)
                            } else if page == Page::Palette && self.palette_previews(viewport) {
                                // Room for Go To's list beside its preview.
                                1120. * (font.size / 12.)
                            } else if page == Page::Palette {
                                // Wide like Raycast, so the query leads the panel.
                                720. * (font.size / 12.)
                            } else {
                                480.
                            })))
                        .max_h((viewport.height - px(32.)).max(px(0.)))
                },
            )
            .when(
                !matches!(
                    page,
                    Page::Keybinds
                        | Page::Themes
                        | Page::DeviceTheme
                        | Page::Palette
                        | Page::CodeSearch
                        | Page::AppUpdate
                        | Page::GitHub
                        | Page::GitLab
                        | Page::AddDevice
                        | Page::AddWsl
                        | Page::Usage(_)
                        | Page::RenameDevice
                        | Page::ForwardPort
                ) && !self.cloud_dialog_open(),
                |panel| {
                    // Dialogs draw their own full-bleed header and footer rules,
                    // so the panel's own inset would cut those rules short.
                    // The device and session pickers scroll their own lists, so
                    // the panel around them stays put; a second scroller there
                    // still had its padding to move through. A session create or
                    // delete form has no inner scroller and keeps the panel's.
                    let lists = matches!(page, Page::Devices | Page::Sessions) && !session_modal;
                    panel
                        .when(!settled && !lists, |panel| panel.overflow_y_scroll())
                        .when(lists, |panel| panel.overflow_hidden())
                        .when(!matches!(page, Page::Dialog(_)), |panel| panel.p(px(6.)))
                },
            )
            .when(
                matches!(
                    page,
                    Page::Keybinds
                        | Page::Themes
                        | Page::DeviceTheme
                        | Page::Palette
                        | Page::CodeSearch
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
            .when(add_device, |panel| {
                panel.h(px(rail::HEIGHT * (font.size / 12.))
                    .min((viewport.height - px(32.)).max(px(0.))))
            })
            .when(page == Page::RemoteDevices, |panel| {
                panel
                    .w((viewport.width - px(24.))
                        .max(px(0.))
                        .min(px(crate::remote::MENU_WIDTH)))
                    .shadow_lg()
            })
            .when(matches!(page, Page::GitHub | Page::GitLab), |panel| {
                panel
                    .w((viewport.width - px(32.)).max(px(0.)).min(px(400.)))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .shadow_lg()
            })
            .when(
                matches!(
                    page,
                    Page::Install
                        | Page::AgentSkill
                        | Page::VersionMismatch
                        | Page::OpenCheckout
                        | Page::OpenLink
                ) || self.cloud_batch_open()
                    || self.remote_dialog_open(),
                |panel| {
                    panel
                        .w((viewport.width - px(24.)).max(px(0.)).min(px(420.)))
                        .max_h((viewport.height - px(24.)).max(px(0.)))
                },
            )
            .when(self.automation_dialog_open(), |panel| {
                panel
                    .w((viewport.width - px(24.)).max(px(0.)).min(px(560.)))
                    .max_h((viewport.height - px(24.)).max(px(0.)))
            })
            .when(
                matches!(
                    page,
                    Page::AppUpdate
                        | Page::AddDevice
                        | Page::AddWsl
                        | Page::RenameDevice
                        | Page::ForwardPort
                ) || self.cloud_dialog_open(),
                |panel| panel.flex().flex_col().overflow_hidden().shadow_lg(),
            )
            .when(page == Page::About, |panel| {
                panel.w((viewport.width - px(24.)).max(px(0.)).min(px(340.)))
            })
            .rounded(px(crate::config::corners::PANEL))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.surface))
            .text_color(rgb(theme.foreground))
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(font.line_height()))
            .when(!rises, |panel| panel.opacity(shown))
            // A popover drops from the pointer that opened it; a dialog rises.
            // Footer and usage panels have slid already, by their own edge.
            .when(
                !footer_anchored && !matches!(page, Page::Usage(_)),
                |panel| panel.top(if dims { lift } else { -lift }),
            )
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
                        .role(Role::MenuItem)
                        .aria_label(menu_item_label(item))
                        .when(Some(index) == self.menu.selected, |row| {
                            row.aria_active_descendant()
                        })
                        .min_h(px(font.line_height() + 12.))
                        .px(px(8.))
                        .row()
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
                        .child(menu_item_label(item))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.activate_menu(item, window, cx);
                        })),
                );
            }
        } else if page == Page::Devices {
            panel = panel.child(self.render_devices(cx));
        } else if page == Page::Sessions {
            panel = panel.child(self.render_sessions(cx));
        } else if let Page::Usage(provider) = page {
            panel = panel.child(self.render_usage_panel(provider, cx));
        } else if page == Page::AddDevice {
            panel = panel.child(self.render_add_device(compact_rail, cx));
        } else if let Some(dialog) = self.render_cloud_dialog(compact_rail, cx) {
            panel = panel.child(dialog);
        } else if page == Page::AddWsl {
            panel = panel.child(self.render_add_wsl(compact_rail, cx));
        } else if page == Page::RemoveWsl {
            panel = panel.child(self.render_remove_wsl(cx));
        } else if page == Page::GitHub {
            panel = panel.child(self.render_github_auth(cx));
        } else if page == Page::GitLab {
            panel = panel.child(self.render_gitlab_page(cx));
        } else if page == Page::RemoteDevices {
            panel = panel.child(self.render_remote_menu(cx));
        } else if page == Page::Workspace {
            // The PR card's text width: the panel's inset and border, and the
            // card's own margin, border, and padding.
            panel = panel.child(self.render_workspace_popover(
                (px(340.).min((viewport.width - px(24.)).max(px(0.))) - px(36.)).max(px(0.)),
                cx,
            ));
        } else if let Page::Dialog(action) = page {
            panel = panel.child(self.render_workspace_dialog(action, cx));
        } else if page == Page::CarryPane {
            panel = panel.child(self.render_carry(cx));
        } else if page == Page::Teleport {
            panel = panel.child(self.render_teleport(cx));
        } else if page == Page::Checkpoints {
            panel = panel.child(self.render_checkpoints(cx));
        } else if page == Page::FanOut {
            panel = panel.child(self.render_fan_out(cx));
        } else if page == Page::Git {
            panel = panel.child(self.render_git_menu(cx));
        } else if page == Page::GitCommit {
            panel = panel.child(self.render_git_commit(cx));
        } else if page == Page::PrReview {
            panel = panel.child(self.render_pr_review(cx));
        } else if page == Page::PrComment {
            panel = panel.child(self.render_pr_comment(cx));
        } else if page == Page::PrMerge {
            panel = panel.child(self.render_pr_merge(cx));
        } else if matches!(
            page,
            Page::Host
                | Page::RenameDevice
                | Page::ForwardPort
                | Page::DeviceTheme
                | Page::RemoveDevice
        ) {
            panel = panel.child(self.render_host_menu(cx));
        } else if page == Page::MoveTab {
            panel = self.render_tab_move_picker(panel, cx);
        } else if matches!(page, Page::Tab | Page::RenameTab) {
            panel = panel.child(self.render_tab_menu(cx));
        } else if page == Page::Group {
            panel = self.render_group_menu(panel, cx);
        } else if page == Page::MovePane {
            panel = self.render_move_picker(panel, cx);
        } else if matches!(
            page,
            Page::Pane | Page::RenamePane | Page::PaneProcesses | Page::KillProcesses
        ) {
            panel = panel.child(self.render_pane_menu(cx));
        } else if page == Page::Keybinds {
            panel = panel.child(self.render_keybinds(cx));
        } else if page == Page::Themes {
            panel = panel.child(self.render_theme_picker(cx));
        } else if page == Page::Palette {
            panel = panel.child(self.render_palette(viewport, cx));
        } else if page == Page::CodeSearch {
            panel = panel.child(self.render_code_search(cx));
        } else if page == Page::ConfirmClose {
            panel = panel.child(self.render_close_confirmation(cx));
        } else if page == Page::AppUpdate {
            panel = panel.child(self.render_app_update(window, cx));
        } else if page == Page::About {
            panel = panel.child(self.render_about(cx));
        } else if page == Page::AgentSkill {
            panel = panel.child(self.render_agent_skill_offer(cx));
        } else if page == Page::VersionMismatch {
            panel = panel.child(self.render_version_mismatch(cx));
        } else if page == Page::WorktreeScript {
            panel = panel.child(self.render_worktree_script(cx));
        } else if page == Page::OpenLink {
            panel = panel.child(self.render_open_link(cx));
        } else if let Some(body) = self.cloud_batch_page(cx) {
            panel = panel.child(body);
        } else if let Some(body) = self.automation_dialog_page(cx) {
            panel = panel.child(body);
        } else if let Some(body) = self.remote_page(cx) {
            panel = panel.child(body);
        } else if page == Page::OpenCheckout {
            panel = panel.child(self.render_open_checkout(cx));
        } else if page == Page::Install {
            panel = panel
                .child(div().p(px(8.)).child(fl!("menu-chrome-install-title")))
                .child(div().p(px(8.)).child(fl!("menu-chrome-install-note")))
                .child(
                    div()
                        .row()
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
                                .child(fl!("menu-chrome-install"))
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
                                .child(fl!("menu-chrome-dismiss"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.dismiss_menu(window, cx);
                                })),
                        ),
                );
        } else {
            panel = panel.child(self.render_release_notes(cx));
        }
        // On Windows pages sit above everything GPUI draws, so the menu says
        // what it covers: a dimmed dialog covers the window, a popover its
        // panel. Tests and native checks read it to find the menu.
        #[cfg(any(windows, test, feature = "integration-test"))]
        let panel = {
            let cover = self.menu.cover.clone();
            if dims {
                cover.set(super::state::Cover::All);
            }
            // A menu on its way out no longer covers anything.
            panel.when(!dims && leaving.is_none(), |panel| {
                panel.child(
                    canvas(
                        // Laid out inside the panel's border; the margin takes
                        // in the border and the start of its shadow.
                        move |bounds, _, _| {
                            cover.set(super::state::Cover::Panel(bounds.dilate(px(COVER_MARGIN))))
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
            })
        };
        // On its way out it takes no clicks: what they would act on has
        // already closed.
        let panel = panel.when(leaving.is_some(), |panel| {
            panel.child(
                div()
                    .id("menu-leaving-shield")
                    .debug_selector(|| "menu-leaving-shield".into())
                    .absolute()
                    .inset_0()
                    .occlude(),
            )
        });
        let placed = if pointer_anchored {
            let position = if page == Page::Git {
                point(self.menu.anchor.x, px(crate::titlebar::HEIGHT + 6.))
            } else {
                self.menu.anchor
            };
            anchored()
                .position(position)
                // A menu hangs toward the end from the pointer, and toward
                // the start from a button at the end: the remote indicator,
                // or the "…" at a strip's right end, which stays there
                // because strips never mirror.
                .anchor(match page {
                    Page::Group => Anchor::TopRight,
                    Page::RemoteDevices => crate::direction::anchor(Anchor::TopRight),
                    _ => crate::direction::anchor(Anchor::TopLeft),
                })
                .snap_to_window_with_margin(Edges::all(px(12.)))
                .child(panel)
                .into_any_element()
        } else if rises {
            self.anchor_footer_panel(
                div().id("menu-rise").overflow_hidden(),
                viewport,
                page,
                px(0.),
            )
            .child(panel)
            .into_any_element()
        } else {
            panel.into_any_element()
        };
        if leaving.is_some() {
            return div()
                .id("menu-leaving")
                .absolute()
                .inset_0()
                .when(dims, |layer| layer.row().items_center().justify_center())
                .child(placed);
        }
        div()
            .id("menu-overlay")
            .debug_selector(|| "menu-overlay".into())
            // Focus stays here while the popup is open, so this is the menu
            // or dialog a screen reader announces.
            .role(page.accessible_role())
            .aria_label(page.accessible_name())
            .absolute()
            .top_0()
            .inset_s(px(0.))
            .bottom_0()
            .inset_e(px(0.))
            .when(dims, |overlay| {
                overlay
                    .row()
                    .items_center()
                    .justify_center()
                    .bg(super::appear::backdrop(
                        theme.background,
                        self.menu.appear.backdrop(now),
                    ))
            })
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
            .on_key_down(cx.listener(Self::menu_key_down))
            .child(placed)
    }
}

/// What the menu shows for `item`, one of [`HerdrWindow::menu_items`]'
/// keys, which stay English as its selectors and dispatch do.
fn menu_item_label(item: &str) -> String {
    match item {
        "settings" => fl!("menu-chrome-settings"),
        "shortcuts" => fl!("menu-chrome-shortcuts"),
        "themes" => fl!("menu-chrome-themes"),
        "increase font size" => fl!("menu-chrome-increase-font-size"), // i18n: data
        "decrease font size" => fl!("menu-chrome-decrease-font-size"), // i18n: data
        "reset font size" => fl!("menu-chrome-reset-font-size"),       // i18n: data
        "command palette" => fl!("menu-chrome-command-palette"),       // i18n: data
        "workspaces" => fl!("menu-chrome-workspaces"),
        "reload GUI config" => fl!("menu-chrome-reload-gui-config"), // i18n: data
        "app updates" => fl!("menu-chrome-app-updates"),             // i18n: data
        "preview app update" => fl!("menu-chrome-preview-app-update"), // i18n: data
        "GitHub sign-in" => fl!("menu-chrome-github-sign-in"),       // i18n: data
        "about" => fl!("menu-chrome-about"),
        "reload daemon config" => fl!("menu-chrome-reload-daemon-config"), // i18n: data
        "update ready" => fl!("menu-chrome-update-ready"),                 // i18n: data
        "what's new" => fl!("menu-chrome-whats-new"),                      // i18n: data
        "detach" => fl!("menu-chrome-detach"),
        "reconnect" => fl!("menu-chrome-reconnect"),
        item => item.to_owned(),
    }
}
