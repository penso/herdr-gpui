//! Painting the window from prepared state. Render reads bounded caches and
//! the latest projection only: it never queries the daemon, touches disk, or
//! starts a process.

use super::HerdrWindow;
use crate::{
    APP_VERSION, CheckForUpdates, Minimize, PlaySound, RunCommand, ShowHerdrNotDetected,
    ShowUpdatePreview, TAB_HEIGHT, TAB_WIDTH, actions::ShowToastPreview,
    config::ClipboardToastPosition, controls::Command, fonts::StyledFont,
    navigation::NavigationTarget, state::ConnectionStatus, terminal::*,
    terminal_painter::FrameBackground, worktree_banner,
};
use gpui::{prelude::*, *};
use herdr_client::ConnectOptions;
use std::time::Duration;

impl Render for HerdrWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.restore_menu_focus(window, cx);
        let opacity = self.paint_opacity();
        let muted = self.theme.readable_chrome(opacity).muted;
        let appearance = crate::config::window_background(opacity);
        if self.last_window_background != Some(appearance) {
            window.set_background_appearance(appearance);
            self.last_window_background = Some(appearance);
        }
        #[cfg(target_os = "macos")]
        super::blur::NativeBlur::update(
            &mut self.native_blur,
            window,
            if opacity < 100 {
                self.config.background_blur_radius
            } else {
                0
            },
        );
        let font = self.config.terminal.font();
        let cell_height = self.config.terminal.line_height();
        self.painter.borrow_mut().set_appearance(
            self.config.terminal.size,
            cell_height,
            self.theme.clone(),
        );
        self.painter.borrow_mut().background_opacity = opacity;
        self.cell_width = self.painter.borrow_mut().cell_width(&font, window, cx);
        // Registers the window for surface-only redraws; see `redraw_terminal`.
        self.surface_signal.read(cx);
        let sidebar = self.sidebar_visible.then(|| {
            crate::sidebar::cached_view(
                &self.sidebar_view,
                self.sidebar_width,
                f32::from(window.viewport_size().width),
            )
        });
        let mut tabs = div()
            .id("tabs")
            .flex()
            .flex_none()
            .h(px((self.config.tabs.size * 1.6 + 4.).max(TAB_HEIGHT)))
            .text_font(&self.config.tabs)
            .text_size(px(self.config.tabs.size))
            .overflow_x_scroll()
            .text_color(rgb(self.theme.foreground))
            .items_center();
        if let Some(snapshot) = &self.live.snapshot {
            for tab in snapshot
                .tabs
                .iter()
                .filter(|t| Some(&t.workspace_id) == snapshot.focused_workspace_id.as_ref())
            {
                let id = tab.tab_id.clone();
                let context_id = id.clone();
                let close_id = id.clone();
                // Selected tabs carry the theme's accent, so the choice reads as
                // primary rather than as the hover tint used elsewhere; the rest
                // recede into the strip, as they do in the reference UI.
                let (background, text) = if tab.focused {
                    let background = self.theme.primary_wash();
                    (background, self.theme.text_on(background))
                } else {
                    (self.theme.surface, muted)
                };
                tabs = tabs.child(
                    div()
                        .id(SharedString::from(format!("tab-{id}")))
                        .debug_selector({
                            let id = id.clone();
                            move || format!("tab-{id}")
                        })
                        .pl(px(12.))
                        // The close button hugs the tab's inner right edge, well
                        // clear of the label it would otherwise crowd.
                        .pr(px(3.))
                        .py(px(2.))
                        // Even cells divided by a single rule, as in the reference UI.
                        .min_w(px(TAB_WIDTH))
                        .border_r_1()
                        .border_color(rgb(self.theme.active))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .cursor_pointer()
                        .bg(crate::config::background(background, opacity))
                        .text_color(rgb(text))
                        .child(tab.label.clone())
                        .child(
                            div()
                                .id("close-tab")
                                .debug_selector({
                                    let id = id.clone();
                                    move || format!("close-tab-{id}")
                                })
                                .size(px(18.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(crate::config::corners::CONTROL))
                                .hover(move |s| s.bg(rgba((text << 8) | 0x24)))
                                .child(
                                    svg()
                                        .path("icons/close.svg")
                                        .debug_selector({
                                            let id = id.clone();
                                            move || format!("close-tab-icon-{id}")
                                        })
                                        .size(px(12.))
                                        .text_color(rgb(text)),
                                )
                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.open_tab_close(&close_id, window, cx);
                                })),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                this.open_tab_menu(&context_id, event.position, window, cx);
                                this.menu.opening_right_click =
                                    this.menu.page == Some(crate::menu::Page::Tab);
                            }),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate(NavigationTarget::Tab(&id), cx);
                            window.focus(&this.focus, cx);
                        })),
                );
            }
        }
        // Paints the frame on screen, which during a focus change is the one
        // presented before it: the terminal area never blanks between two
        // projections. What the client knows to be current stays in `live`.
        let surface = self.presentation.frame(&self.live);
        let entity = cx.entity();
        let paint_entity = entity.clone();
        let focus = self.focus.clone();
        let menu_open = self.menu.page.is_some();
        let cell_width = self.cell_width;
        let painter = self.painter.clone();
        // The highlight is grid coordinates, so it paints with the frame that
        // owns the cells rather than being recomputed from the pointer here.
        let selection = self.selection.clone();
        // The IME composition paints inline at the input cursor; a menu's
        // text field shows its own.
        // It anchors to the live surface, as the IME's candidate window does,
        // so a retained frame never separates the text from the window.
        let marked = (!menu_open && !self.marked.is_empty())
            .then(|| (self.marked.clone(), self.live.surface.clone()));
        self.hovered_terminal_link =
            self.terminal_link_hovered(window.mouse_position(), window.modifiers());
        self.split_cursor = self.split_cursor_at(window.mouse_position());
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
            .when_some(self.split_cursor, |terminal, cursor| {
                terminal.cursor(cursor)
            })
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                let split_cursor = this.split_cursor_at(event.position);
                if split_cursor != this.split_cursor {
                    this.split_cursor = split_cursor;
                    cx.notify();
                }
                // A border is not the application's to hover.
                if split_cursor.is_none() {
                    this.terminal_mouse_hover(event, cx);
                }
                let hovered = this.terminal_link_hovered(event.position, event.modifiers);
                if hovered != this.hovered_terminal_link {
                    this.hovered_terminal_link = hovered;
                    cx.notify();
                }
            }))
            // Holding the link modifier over a link in a mouse-reporting
            // application changes what a click does, so the pointer follows.
            .on_modifiers_changed(
                cx.listener(|this, event: &ModifiersChangedEvent, window, cx| {
                    let hovered =
                        this.terminal_link_hovered(window.mouse_position(), event.modifiers);
                    if hovered != this.hovered_terminal_link {
                        this.hovered_terminal_link = hovered;
                        cx.notify();
                    }
                }),
            )
            .on_click(cx.listener(Self::open_terminal_link))
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .bg(crate::config::background(self.theme.background, opacity))
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
                    this.menu.opening_right_click = this.menu.page == Some(crate::menu::Page::Pane);
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if this.scrollbar_mouse_down(event, cx)
                        || this.split_mouse_down(event, cx)
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
                                        || this.split_mouse_move(event, cx)
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
                                    // Global: the overlay occludes the opener, and release
                                    // may also precede the overlay's first frame.
                                    if matches!(
                                        event.button,
                                        MouseButton::Left | MouseButton::Right
                                    ) {
                                        this.menu.opening_right_click = false;
                                    }
                                    if this.scrollbar_mouse_up(event, cx)
                                        || this.split_mouse_up(event, cx)
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
                            crate::input::TerminalInputHandler::new(
                                bounds,
                                paint_entity.clone(),
                                menu_open,
                            ),
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
                                FrameBackground::Window,
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
                                    FrameBackground::Popup,
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
                        if let Some((marked, live)) = &marked {
                            let live = live.as_deref();
                            painter.borrow().paint_composition(
                                marked,
                                input_cursor_bounds(live, bounds.origin, cell_width, cell_height)
                                    .origin,
                                input_area(live, bounds, cell_width, cell_height),
                                &font,
                                window,
                            );
                        }
                    },
                )
                .size_full(),
            )
            // Direct feedback for the user's own gesture, not a daemon notice:
            // it sits over the cells it copied and needs no dismissing.
            .when_some(self.flash.as_ref(), |terminal, (flash, _)| {
                use ClipboardToastPosition::*;
                let position = self.config.clipboard_toast.position;
                terminal.child(
                    div()
                        .absolute()
                        .map(|row| match position {
                            TopLeft | TopCenter | TopRight => row.top(px(12.)),
                            BottomLeft | BottomCenter | BottomRight => row.bottom(px(12.)),
                        })
                        .map(|row| match position {
                            TopLeft | BottomLeft => row.justify_start(),
                            TopCenter | BottomCenter => row.justify_center(),
                            TopRight | BottomRight => row.justify_end(),
                        })
                        // The pane's own padding is not part of the terminal:
                        // the flash spans the cells, so centering centers on
                        // them and a corner is the corner of the grid.
                        .left(px(sidebar_gap))
                        .right_0()
                        .px(px(12.))
                        .flex()
                        .overflow_hidden()
                        .child(
                            div()
                                .debug_selector(|| "flash".into())
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .px(px(12.))
                                .py(px(6.))
                                .rounded(px(crate::config::corners::CONTROL))
                                .border_1()
                                .border_color(rgb(flash.accent(&self.theme)))
                                .bg(crate::config::background(self.theme.surface, opacity))
                                .text_color(rgb(self.theme.foreground))
                                .child(
                                    div()
                                        .size(px(6.))
                                        .flex_none()
                                        .rounded_full()
                                        .bg(rgb(flash.accent(&self.theme))),
                                )
                                .child(div().truncate().child(flash.text.clone())),
                        ),
                )
            });
        let status = (!matches!(self.live.status, ConnectionStatus::Connected)
            || self.local_error.is_some()
            || self.live.error.is_some())
        .then(|| self.live.status_text(self.local_error.as_deref()));
        div()
            .on_action(cx.listener(|this, action: &crate::actions::SetLayout, _, cx| {
                this.set_layout(action.mode, cx);
            }))
            .on_action(cx.listener(|this, action: &RunCommand, window, cx| {
                this.command(action.command, window, cx);
            }))
            .on_action(cx.listener(|_, _: &Minimize, window, _| {
                window.minimize_window();
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
            .on_action(cx.listener(|this, _: &crate::actions::ShowUpdateDownloadPreview, window, cx| {
                this.open_update_progress_preview(
                    crate::updater::State::Downloading { received: 50_000_000, total: 100_000_000 },
                    window,
                    cx,
                );
            }))
            .on_action(cx.listener(|this, _: &crate::actions::ShowUpdateHomebrewPreview, window, cx| {
                this.open_update_progress_preview(
                    crate::updater::State::Upgrading { detail: "Refreshing Homebrew metadata with brew update...".into() },
                    window,
                    cx,
                );
            }))
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
            // Child regions each paint their own readable background. A full-
            // window fill would compound their alpha and hide the desktop.
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
                div()
                    .debug_selector(|| "window-body".into())
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .children(sidebar)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                             .flex_1()
                             .min_w_0()
                             .min_h_0()
                            .child(
                                div()
                                    .flex()
                                    .flex_none()
                                    .text_color(rgb(self.theme.foreground))
                                    // Tabs size to their content and shrink when the
                                    // row is full, so the button sits after the last
                                    // tab instead of at the far right of the window.
                                    .child(tabs.flex_shrink_1().min_w_0())
                                    .child(
                                        div()
                                            .id("new-tab")
                                            .debug_selector(|| "new-tab".into())
                                            .bg(crate::config::background(self.theme.surface, opacity))
                                            .w(px(34.))
                                            .min_h(px(TAB_HEIGHT))
                                            .border_r_1()
                                            .border_color(rgb(self.theme.active))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(self.theme.active)))
                                            .child(
                                                svg()
                                                    .path("icons/plus.svg")
                                                    .debug_selector(|| "new-tab-icon".into())
                                                    .size(px(14.))
                                                    // Quiet like the unselected tabs beside it.
                                                    .text_color(rgb(muted)),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.command(Command::Tab, window, cx)
                                            })),
                                    )
                                    // Each region owns one background; filling the
                                    // wrapper too would compound translucent tabs.
                                    .child(
                                        div()
                                            .debug_selector(|| "tab-strip-remainder".into())
                                            .flex_1()
                                            .h_full()
                                            .bg(crate::config::background(self.theme.surface, opacity)),
                                    ),
                            )
                            .child(terminal)
                            .child(
                div()
                    .id("connection-status")
                    .debug_selector(|| "connection-status".into())
                    .flex()
                    .flex_none()
                    .h(px((self.config.ui.size * 1.5 + 4.).max(22.)))
                    .overflow_hidden()
                    .items_center()
                    .gap(px(6.))
                    .px_3()
                    .bg(crate::config::background(self.theme.surface, opacity))
                    .text_color(rgb(self.theme.foreground))
                    .children(self.render_usage(cx))
                    .when(!self.live.status.is_connected(), |bar| bar.child(
                        if matches!(self.live.status, ConnectionStatus::StartingDaemon) {
                            div()
                                .size(px(8.))
                                .flex_none()
                                .rounded_full()
                                .bg(rgb(self.theme.palette[3]))
                                .with_animation(
                                    "daemon-starting-loader",
                                    Animation::new(Duration::from_secs(1)).repeat(),
                                    |dot, delta| {
                                        dot.opacity(
                                            0.3 + 0.7 * (delta * std::f32::consts::PI).sin(),
                                        )
                                    },
                                )
                                .into_any_element()
                        } else {
                            div()
                                .size(px(6.))
                                .flex_none()
                                .rounded_full()
                                .bg(rgb(self.theme.palette[1]))
                                .into_any_element()
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .when_some(status, |row, status| row.child(
                                div().debug_selector(|| "connection-message".into()).child(status)
                            )),
                    )
                    .child(
                        div()
                                    .id("status-theme")
                                    .debug_selector(|| "status-theme".into())
                                    .flex_shrink_1()
                                    .min_w(px(33.))
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .px_2()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(self.theme.active)))
                            .child(
                                svg()
                                    .path("icons/theme.svg")
                                    .size(px(12.))
                                    .flex_none()
                                    .text_color(rgb(self.theme.foreground)),
                            )
                                    .child(div().min_w_0().truncate().child("Theme"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_theme_picker(window, cx);
                            })),
                    )
                    .child(
                        div()
                                    .id("status-keybinds")
                                    .debug_selector(|| "status-keybinds".into())
                                    .flex_shrink_1()
                                    .min_w(px(33.))
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .px_2()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(self.theme.active)))
                            .child(
                                svg()
                                    .path("icons/keyboard.svg")
                                    .size(px(12.))
                                    .flex_none()
                                    .text_color(rgb(self.theme.foreground)),
                            )
                                    .child(div().min_w_0().truncate().child("Shortcuts"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_keybinds(window, cx);
                            })),
                    )
                    .child(
                        div()
                                    .id("report-issue")
                                    .debug_selector(|| "report-issue".into())
                                    .flex_shrink_1()
                                    .min_w(px(33.))
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .px_2()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(self.theme.active)))
                            .child(
                                div()
                                    .size(px(12.))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .border_1()
                                    .border_color(rgb(self.theme.foreground))
                                    .child(
                                        div()
                                            .size(px(3.))
                                            .rounded_full()
                                            .bg(rgb(self.theme.foreground)),
                                    ),
                            )
                                    .child(div().min_w_0().truncate().child("Report issue"))
                            .on_click(|_, _, cx| {
                                cx.open_url(&format!(
                                    "https://github.com/penso/herdr-gpui/issues/new?template=bug_report.yml&version={}",
                                    APP_VERSION.replace('+', "%2B"),
                                ));
                            }),
                    )
                    .child(
                        div()
                            .id("status-version")
                            .debug_selector(|| "status-version".into())
                            .flex_none()
                            .whitespace_nowrap()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(self.theme.active)))
                            // A waiting update is the one status here worth
                            // interrupting for, so it takes the accent color
                            // the rest of the chrome reserves for chosen rows.
                            .text_color(rgb(if self.updater.update_available() {
                                self.theme.primary()
                            } else {
                                muted
                            }))
                            .child(if self.updater.update_available() {
                                "Update available"
                            } else {
                                APP_VERSION
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_app_update(false, window, cx);
                            })),
                    ),
                            ),
                    ),
            )
            .children(self.render_toasts(window, cx))
            .children(self.render_file_transfer(window, cx))
            .when(self.menu.page.is_some(), |root| {
                root.child(self.render_menu(window, cx))
            })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use core::prelude::v1::test;

    #[gpui::test]
    fn tab_regions_paint_exactly_one_background(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        for setting in [0, 50, 100] {
            view.update(cx, |view, cx| {
                view.config.background_opacity = setting;
                std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap())
                    .focused_workspace_id = Some("w0".into());
                cx.notify();
            });
            cx.update(|window, cx| {
                window.resize(size(px(1200.), px(780.)));
                window.draw(cx).clear(cx);
            });
            for selector in ["tab-t0", "tab-t1", "new-tab", "tab-strip-remainder"] {
                let bounds = cx.debug_bounds(selector).unwrap();
                // Below the text/icons and clear of the right-hand border.
                let sample = point(bounds.left() + px(2.), bounds.bottom() - px(2.));
                cx.update(|window, cx| {
                    let sample = sample.scale(window.scale_factor());
                    let backgrounds: Vec<_> = window
                        .painted_quads()
                        .into_iter()
                        .filter(|quad| {
                            quad.bounds.contains(&sample)
                                && quad.content_mask.bounds.contains(&sample)
                                && quad.background.as_solid().is_some_and(|color| color.a > 0.)
                        })
                        .collect();
                    assert_eq!(
                        backgrounds.len(),
                        1,
                        "{selector} at {setting}%: {backgrounds:?}"
                    );
                    let expected = crate::config::background(0, view.read(cx).paint_opacity()).a;
                    assert_eq!(backgrounds[0].background.as_solid().unwrap().a, expected);
                });
            }
        }
    }
}
