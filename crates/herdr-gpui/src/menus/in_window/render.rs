//! Menu bar and bounded, scrollable dropdown presentation.

use super::*;
use crate::fonts::StyledFont;
use gpui::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum BarLayer {
    Base,
    Overlay,
}

impl HerdrWindow {
    fn compact_application_bar(&self, window: &Window) -> bool {
        // Reserve space for all three window buttons and their padding before
        // deciding whether the headings fit beside them.
        let controls = if crate::titlebar::show_controls(self.config.show_window_controls, window) {
            112.
        } else {
            0.
        };
        window.viewport_size().width < px(560. * self.config.ui.size / 12. + controls)
    }

    pub(crate) fn render_application_bar(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.application_bar(BarLayer::Base, window, cx)
    }

    fn application_bar(
        &self,
        layer: BarLayer,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if cx.try_global::<super::super::Headings>().is_none() {
            super::super::install(cx);
        }
        let headings = &cx.global::<super::super::Headings>().0;
        let compact = self.compact_application_bar(window);
        let entries: Vec<_> = if compact {
            vec![(None, SharedString::from("Menu"), false)]
        } else {
            headings
                .iter()
                .enumerate()
                .map(|(i, (name, disabled))| (Some(i), name.clone(), *disabled))
                .collect()
        };
        let geometry = self.menu.application_bar.bar.clone();
        let buttons = &self.menu.application_bar.buttons;
        // Only the in-flow bar owns anchor measurements. Its overlay copy
        // must never replace those with geometry from a different frame.
        let measure = layer == BarLayer::Base;
        if measure {
            buttons
                .borrow_mut()
                .resize(entries.len(), Bounds::default());
        }
        let font = &self.config.ui;
        let theme = &self.theme;
        let height = if crate::titlebar::show_controls(self.config.show_window_controls, window) {
            (font.line_height() + 8.).max(crate::titlebar::HEIGHT)
        } else {
            font.line_height() + 8.
        };
        let bar = div()
            .id("application-menu-bar")
            .debug_selector(|| "application-menu-bar".into())
            // The overlay's copy repeats this bar for the pointer only.
            .when(measure, |bar| {
                bar.role(Role::MenuBar).aria_label("Menu bar")
            })
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .w_full()
            .h(px(height))
            .px(px(6.))
            // The overlay copy lies on the one in the window, so only that
            // one takes the window's translucency.
            .bg(match layer {
                BarLayer::Base => self.fill().panel(theme.surface),
                BarLayer::Overlay => rgb(theme.surface).into(),
            })
            .border_b_1()
            .border_color(rgb(theme.active))
            .text_font(font)
            .text_size(px(font.size))
            .text_color(rgb(theme.foreground))
            .occlude()
            .on_click(|_, _, cx| cx.stop_propagation())
            .when(measure, |bar| {
                bar.child(
                    canvas(move |bounds, _, _| geometry.set(bounds), |_, _, _, _| {})
                        .absolute()
                        .inset_0(),
                )
            })
            .children(
                entries
                    .into_iter()
                    .enumerate()
                    .map(|(slot, (index, name, disabled))| {
                        let buttons = buttons.clone();
                        let selector = format!("application-menu-{name}");
                        let view = cx.entity().downgrade();
                        let active = self
                            .menu
                            .application
                            .as_ref()
                            .is_some_and(|menu| compact || menu.path.first().copied() == index);
                        div()
                            .id(SharedString::from(selector.clone()))
                            .debug_selector(move || selector.clone())
                            .when(measure, |button| {
                                let heading = button.role(Role::MenuItem).aria_label(name.clone());
                                let heading = crate::accessibility::disabled(heading, disabled);
                                if disabled {
                                    return heading;
                                }
                                heading
                                    .aria_expanded(active)
                                    .on_a11y_action(
                                        AccessibleAction::Expand,
                                        crate::accessibility::update(
                                            view.clone(),
                                            move |this, window, cx| {
                                                this.open_application_menu(index, window, cx)
                                            },
                                        ),
                                    )
                                    .on_a11y_action(
                                        AccessibleAction::Collapse,
                                        crate::accessibility::update(view, |this, window, cx| {
                                            this.dismiss_menu(window, cx)
                                        }),
                                    )
                            })
                            .relative()
                            .flex_none()
                            .h_full()
                            .px(px(10.))
                            // Menu presses must never start a window move or
                            // open the compositor's title-bar context menu.
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                            .flex()
                            .items_center()
                            .rounded(px(crate::config::corners::CONTROL))
                            .when(active, |button| button.bg(rgb(theme.active)))
                            .when(disabled, |button| button.text_color(rgb(theme.muted)))
                            .when(!disabled, |button| {
                                button.cursor_pointer().hover(|s| s.bg(rgb(theme.active)))
                            })
                            .child(name)
                            .when(measure, |button| {
                                button.child(
                                    canvas(
                                        move |bounds, _, _| {
                                            if let Some(button) = buttons.borrow_mut().get_mut(slot)
                                            {
                                                *button = bounds;
                                            }
                                        },
                                        |_, _, _, _| {},
                                    )
                                    .absolute()
                                    .inset_0(),
                                )
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                if disabled {
                                    return;
                                }
                                if this.menu.application.as_ref().is_some_and(|menu| {
                                    compact || menu.path.first().copied() == index
                                }) {
                                    this.dismiss_menu(window, cx);
                                } else {
                                    this.open_application_menu(index, window, cx);
                                }
                            }))
                            .on_hover(cx.listener(move |this, hovered, window, cx| {
                                if *hovered
                                    && !disabled
                                    && !compact
                                    && this
                                        .menu
                                        .application
                                        .as_ref()
                                        .is_some_and(|menu| menu.path.first().copied() != index)
                                {
                                    this.open_application_menu(index, window, cx);
                                }
                            }))
                    }),
            )
            .child(div().flex_1().h_full().min_w(px(24.)))
            .children(crate::titlebar::controls(
                self.config.show_window_controls,
                window,
                theme,
                |window, _| {
                    window.remove_window();
                },
            ));
        // Pass controls_enabled so users who hide the WM's title bar can still
        // drag the window from the menu bar.
        crate::titlebar::movable_with_controls(bar, window, self.config.show_window_controls)
    }

    /// A menu item's shortcut in keymap spelling, the way the palette and the
    /// Keyboard Shortcuts page show it. A `cmd-` the user configured is
    /// named for this desktop's platform key, as GPUI's `unparse` names it.
    pub(super) fn application_shortcut(&self, action: &dyn Action, cx: &App) -> String {
        if let Some(command) = action.as_any().downcast_ref::<crate::RunCommand>() {
            let platform = if cfg!(windows) { "win-" } else { "super-" };
            return self
                .keymap()
                .shortcuts(command.command)
                .next()
                .unwrap_or_default()
                .replace("cmd-", platform);
        }
        cx.key_bindings()
            .borrow()
            .bindings_for_action(action)
            .last()
            .map(|binding| {
                binding
                    .keystrokes()
                    .iter()
                    .map(KeybindingKeystroke::unparse)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    }

    pub(crate) fn render_application_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut overlay = div()
            .id("application-menu-overlay")
            .debug_selector(|| "application-menu-overlay".into())
            .absolute()
            .inset_0()
            .occlude()
            .track_focus(&self.menu.focus)
            .on_key_down(cx.listener(Self::application_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.dismiss_menu(window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.dismiss_menu(window, cx);
                }),
            )
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation());
        let Some(menu) = &self.menu.application else {
            return overlay;
        };
        // Focus stays on the overlay while the arrows move the highlight, so
        // the overlay is the menu and the highlighted row its active item.
        overlay = overlay.role(Role::Menu).aria_label(
            menu.current()
                .map_or_else(|| SharedString::from("Menu"), |menu| menu.name.clone()),
        );
        let bar = self.menu.application_bar.bar.get();
        let content = self.menu.application_bar.content.get();
        let compact = self.compact_application_bar(window);
        let button = self
            .menu
            .application_bar
            .buttons
            .borrow()
            .get(if compact {
                0
            } else {
                menu.path.first().copied().unwrap_or(0)
            })
            .copied()
            .unwrap_or(bar);
        let font = &self.config.ui;
        let theme = &self.theme;
        let width = px(380. * font.size / 12.).min((content.size.width - px(16.)).max(px(0.)));
        let top = bar.bottom();
        let current = menu.current();
        let items = current.map(|menu| menu.items.clone()).unwrap_or_else(|| {
            menu.menus
                .iter()
                .cloned()
                .map(OwnedMenuItem::Submenu)
                .collect()
        });
        let mut rows = div()
            .id("application-menu-rows")
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&menu.scroll);
        for (index, item) in items.into_iter().enumerate() {
            let (name, disabled, checked, detail) = match item {
                OwnedMenuItem::Separator => {
                    rows = rows.child(
                        div()
                            .flex_none()
                            .h(px(1.))
                            .my(px(4.))
                            .mx(px(8.))
                            .bg(rgb(theme.active)),
                    );
                    continue;
                }
                OwnedMenuItem::Submenu(menu) => {
                    (menu.name.to_string(), menu.disabled, false, "›".to_owned())
                }
                OwnedMenuItem::Action {
                    name,
                    action,
                    disabled,
                    checked,
                    ..
                } => {
                    let detail = self.application_shortcut(action.as_ref(), cx);
                    (name, disabled, checked, detail)
                }
                OwnedMenuItem::SystemMenu(menu) => {
                    (menu.name.to_string(), true, false, String::new())
                }
            };
            let selector = format!("application-item-{name}");
            let spoken = match detail.as_str() {
                "" | "›" => name.clone(),
                shortcut => format!("{name}, {shortcut}"),
            };
            let item = div()
                .id(("application-item", index))
                .role(if checked {
                    Role::MenuItemCheckBox
                } else {
                    Role::MenuItem
                })
                .aria_label(spoken)
                .when(checked, |item| item.aria_toggled(Toggled::True))
                .when(menu.selected == Some(index), |item| {
                    item.aria_active_descendant()
                });
            rows = rows.child(
                crate::accessibility::disabled(item, disabled)
                    .debug_selector(move || selector.clone())
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.))
                    .px(px(8.))
                    .min_h(px(font.line_height() + 10.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .when(menu.selected == Some(index), |row| {
                        row.bg(rgb(theme.active))
                    })
                    .when(disabled, |row| row.text_color(rgb(theme.muted)))
                    .when(!disabled, |row| row.cursor_pointer())
                    .child(
                        div()
                            .flex_none()
                            .w(px(14.))
                            .child(if checked { "✓" } else { "" }),
                    )
                    .child(div().flex_1().min_w_0().overflow_hidden().child(name))
                    .child(div().flex_none().text_color(rgb(theme.muted)).child(detail))
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered
                            && !disabled
                            && let Some(menu) = &mut this.menu.application
                        {
                            menu.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.application_choose(index, window, cx);
                    })),
            );
        }
        let panel = div()
            .id("application-menu-panel")
            .debug_selector(|| "application-menu-panel".into())
            .relative()
            .w(width)
            .max_h((content.bottom() - top - px(8.)).max(px(0.)))
            .flex()
            .flex_col()
            .overflow_hidden()
            .p(px(5.))
            .bg(rgb(theme.surface))
            .text_color(rgb(theme.foreground))
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(font.line_height()))
            .border_1()
            .border_color(rgb(theme.active))
            .rounded(px(crate::config::corners::PANEL))
            .shadow_lg()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .when_some(current, |panel, current| {
                panel.child(
                    div()
                        .id("application-menu-back")
                        .debug_selector(|| "application-menu-back".into())
                        .role(Role::MenuItem)
                        .aria_label(format!("Back to {}", current.name))
                        .flex_none()
                        .p(px(8.))
                        .cursor_pointer()
                        .child(format!("‹  {}", current.name))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.application_back(cx);
                        })),
                )
            })
            .child(rows);
        // On Windows pages sit above everything GPUI draws, so the menu says
        // what its panel covers. Tests and native checks read it to find the
        // menu.
        #[cfg(any(windows, test, feature = "integration-test"))]
        let panel = {
            let cover = self.menu.cover.clone();
            panel.child(
                canvas(
                    move |bounds, _, _| {
                        cover.set(crate::menu::Cover::Panel(bounds.dilate(px(8.))));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
        };
        // Repeat the bar above the occluding layer, so switching headings never
        // clicks through the dismissal layer into unrelated window controls.
        // The overlay fills the same content root as the underlying bar. Lay out
        // this copy there too: using cached bounds would feed its stale geometry
        // back into the next frame after a resize or decoration inset change.
        overlay = overlay.child(self.application_bar(BarLayer::Overlay, window, cx));
        overlay.child(
            anchored()
                .position(point(
                    button
                        .left()
                        .min((content.right() - width - px(8.)).max(content.left() + px(8.)))
                        .max(content.left() + px(8.)),
                    top,
                ))
                .snap_to_window_with_margin(Edges::all(px(8.)))
                // Moving to another heading keeps the menu still: only one
                // opened from the closed bar drops in.
                .child(crate::motion::pop_in(panel, "application-menu")),
        )
    }
}
