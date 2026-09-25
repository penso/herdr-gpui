//! Painting the window from prepared state. Render reads bounded caches and
//! the latest projection only: it never queries the daemon, touches disk, or
//! starts a process.

use super::HerdrWindow;
use crate::{
    CheckForUpdates, PlaySound, RunCommand, ShowHerdrNotDetected, ShowUpdatePreview,
    actions::ShowToastPreview, fonts::StyledFont, terminal::*, worktree_banner,
};
use gpui_kit::{component::Root, prelude::*, *};
use herdr_client::ConnectOptions;

impl Render for HerdrWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Theme previews and config reloads only change `self.theme`; follow
        // them here so kit components repaint in the same colors. The kit
        // theme is app-global, so only the active window drives it: windows
        // with different themes would otherwise overwrite each other forever.
        if window.is_window_active() {
            crate::kit_theme::sync(&self.theme, &self.config, cx);
        }
        let font = self.config.terminal.font();
        let cell_height = self.config.terminal.line_height();
        self.painter.borrow_mut().set_appearance(
            self.config.terminal.size,
            cell_height,
            self.theme.clone(),
        );
        self.cell_width = self.painter.borrow_mut().cell_width(&font, window, cx);
        // Registers the window for surface-only redraws; see `redraw_terminal`.
        self.surface_signal.read(cx);
        // The sidebar and the main column, divided by the kit's resize handle.
        let body = self.sidebar_body(window, cx);
        let tabs = self.render_tab_strip(cx);
        // Paints the frame on screen, which during a focus change is the one
        // presented before it: the terminal area never blanks between two
        // projections. What the client knows to be current stays in `live`.
        let surface = self.presentation.frame(&self.live);
        let entity = cx.entity();
        let paint_entity = entity.clone();
        let focus = self.focus.clone();
        let cell_width = self.cell_width;
        let painter = self.painter.clone();
        // The highlight is grid coordinates, so it paints with the frame that
        // owns the cells rather than being recomputed from the pointer here.
        let selection = self.selection.clone();
        self.hovered_terminal_link = self.terminal_link_at(window.mouse_position()).is_some()
            && (window.modifiers().shift
                || self
                    .terminal_mouse_at(window.mouse_position())
                    .is_none_or(|hit| !hit.mouse_reporting));
        // Pad the terminal itself: the canvas bounds that painting, hit testing,
        // and IME placement all read then already exclude the gap.
        let sidebar_gap = if self.sidebar_visible {
            self.config.layout.sidebar_gap
        } else {
            0.
        };
        let terminal = div()
            .id("terminal")
            .debug_selector(|| "terminal".into())
            .pl(px(sidebar_gap))
            .when(self.hovered_terminal_link, |terminal| {
                terminal.cursor_pointer()
            })
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                this.terminal_mouse_hover(event, cx);
                let hovered = this.terminal_link_at(event.position).is_some()
                    && (event.modifiers.shift
                        || this
                            .terminal_mouse_at(event.position)
                            .is_none_or(|hit| !hit.mouse_reporting));
                if hovered != this.hovered_terminal_link {
                    this.hovered_terminal_link = hovered;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(Self::open_terminal_link))
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .bg(rgb(self.theme.background))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            // A selection is copied when it is released, so the terminal has
            // nothing for Cut, Copy, or Select All to act on.
            .on_action(cx.listener(|this, _: &crate::actions::Paste, _, cx| this.paste(cx)))
            .on_scroll_wheel(cx.listener(Self::scroll_wheel))
            .on_drop(cx.listener(Self::drop_terminal_files))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.terminal_mouse_down(event, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if this.terminal_mouse_down(event, window, cx) {
                        return;
                    }
                    cx.stop_propagation();
                    this.open_pane_menu_at(event.position, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if this.scrollbar_mouse_down(event, cx)
                        || this.terminal_mouse_down(event, window, cx)
                    {
                        return;
                    }
                    this.pressed_terminal_link = this
                        .terminal_link_at(event.position)
                        .map(|url| (url, event.position));
                    if this.menu.page.is_some() {
                        return;
                    }
                    // A press on a link may still turn into a drag across it,
                    // so the selection starts either way; the click that opens
                    // the link is the one that never left its half-cell.
                    this.begin_selection(event.position, cx);
                    if this.pressed_terminal_link.is_some() {
                        cx.stop_propagation();
                        return;
                    }
                    window.focus(&this.focus, cx);
                    if this.input_ready()
                        && let Some(surface) = &this.live.surface
                    {
                        let pane = pane_at(
                            surface,
                            this.bounds,
                            event.position,
                            this.cell_width,
                            this.config.terminal.line_height(),
                        )
                        .map(str::to_owned);
                        if let Some(id) = pane {
                            this.focus_clicked_pane(&id, cx);
                        }
                    }
                }),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |this, _| {
                            this.bounds = bounds;
                            this.options = ConnectOptions {
                                surface_size: viewport(
                                    bounds.size.width.to_f64() as f32,
                                    bounds.size.height.to_f64() as f32,
                                    cell_width,
                                    cell_height,
                                ),
                                cell_width_px: cell_width.round().max(1.) as u32,
                                cell_height_px: cell_height.round().max(1.) as u32,
                            };
                            this.resize();
                        });
                    },
                    move |bounds, _, window, cx| {
                        // Capture movement outside the terminal too, before any
                        // element can stop propagation of a drag-away event.
                        let entity = paint_entity.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                entity.update(cx, |this, cx| {
                                    if this.scrollbar_mouse_move(event, cx)
                                        || this.terminal_mouse_move(event, cx)
                                    {
                                        cx.stop_propagation();
                                        return;
                                    }
                                    if this.pressed_terminal_link.as_ref().is_some_and(
                                        |(_, position)| {
                                            (event.position.x - position.x).abs() > px(4.)
                                                || (event.position.y - position.y).abs() > px(4.)
                                        },
                                    ) {
                                        this.pressed_terminal_link = None;
                                    }
                                    // A drag that leaves the terminal keeps
                                    // selecting, and hover work elsewhere stays
                                    // out of the gesture.
                                    if this.extend_selection(event.position, cx) {
                                        cx.stop_propagation();
                                    }
                                });
                            }
                        });
                        let released = paint_entity.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                released.update(cx, |this, cx| {
                                    if this.scrollbar_mouse_up(event, cx)
                                        || this.terminal_mouse_up(event, cx)
                                        || (event.button == MouseButton::Left
                                            && !cx.has_active_drag()
                                            && this.release_selection(cx))
                                    {
                                        cx.stop_propagation();
                                    }
                                });
                            }
                        });
                        window.handle_input(
                            &focus,
                            crate::input::TerminalInputHandler::new(bounds, paint_entity.clone()),
                            cx,
                        );
                        if let Some(surface) = &surface {
                            // The highlight belongs to the frame that owns the
                            // cells, so only one of the two paints it.
                            let highlight = |owned: bool| {
                                selection
                                    .as_ref()
                                    .filter(|_| owned)
                                    .map(|selection| {
                                        selection.rows(surface, cell_width, cell_height).collect()
                                    })
                                    .unwrap_or_default()
                            };
                            let panes: Vec<_> = highlight(
                                selection
                                    .as_ref()
                                    .is_some_and(|selection| selection.in_panes()),
                            );
                            painter.borrow_mut().paint_frame(
                                &surface.frame,
                                bounds.origin,
                                cell_width,
                                &font,
                                &panes,
                                &surface.panes,
                                window,
                                cx,
                            );
                            if let Some(popup) = &surface.popup {
                                let offset = popup_origin(
                                    &surface.frame,
                                    &popup.frame,
                                    cell_width,
                                    cell_height,
                                );
                                let rows: Vec<_> =
                                    highlight(selection.as_ref().is_some_and(|selection| {
                                        selection.in_popup(&popup.terminal_id)
                                    }));
                                painter.borrow_mut().paint_frame(
                                    &popup.frame,
                                    bounds.origin + offset,
                                    cell_width,
                                    &font,
                                    &rows,
                                    &[],
                                    window,
                                    cx,
                                );
                            }
                        }
                    },
                )
                .size_full(),
            )
            .children(self.render_flash(sidebar_gap));
        let status_bar = self.render_status_bar(cx);
        self.sync_toasts(window, cx);
        div()
            .on_action(cx.listener(|this, action: &RunCommand, window, cx| {
                this.command(action.command, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowHerdrNotDetected, window, cx| {
                this.show_install_modal(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CheckForUpdates, window, cx| {
                this.open_app_update(false, window, cx);
                this.updater.check();
            }))
            .on_action(cx.listener(|this, _: &ShowUpdatePreview, window, cx| {
                this.open_app_update(true, window, cx);
            }))
            .on_action(cx.listener(
                |this, _: &crate::actions::ShowUpdateDownloadPreview, window, cx| {
                    this.open_update_progress_preview(
                        crate::updater::State::Downloading {
                            received: 50_000_000,
                            total: 100_000_000,
                        },
                        window,
                        cx,
                    );
                },
            ))
            .on_action(cx.listener(
                |this, _: &crate::actions::ShowUpdateHomebrewPreview, window, cx| {
                    this.open_update_progress_preview(
                        crate::updater::State::Upgrading {
                            detail: "Refreshing Homebrew metadata with brew update...".into(),
                        },
                        window,
                        cx,
                    );
                },
            ))
            .on_action(cx.listener(|this, action: &ShowToastPreview, _, cx| {
                this.show_toast_preview(action.kind, cx);
            }))
            .on_action(cx.listener(|this, _: &PlaySound, _, _| {
                this.sound.preview();
            }))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(self.theme.background))
            .text_color(rgb(self.theme.foreground))
            .text_font(&self.config.ui)
            .text_size(px(self.config.ui.size))
            .child(self.render_titlebar(cx))
            .children(worktree_banner::render(
                env!("HERDR_BUILD_WORKTREE") == "1",
                env!("HERDR_BUILD_BRANCH"),
                env!("HERDR_BUILD_PR"),
            ))
            .child(
                body.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .child(tabs)
                        .child(terminal)
                        .child(status_bar),
                ),
            )
            .children(self.render_file_transfer(window, cx))
            .children(Root::render_notification_layer(window, cx))
            // Popup menus and dialogs; see `menu::OverlayLayer`.
            .child(self.menu.layer.clone())
    }
}
