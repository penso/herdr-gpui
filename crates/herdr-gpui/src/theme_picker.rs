use crate::{HerdrWindow, config::Theme, menu::Page, search_input::SearchInput};
use gpui::{prelude::*, *};

pub(super) struct ThemePicker {
    pub search: Entity<SearchInput>,
    names: Vec<String>,
    pub(super) filtered: Vec<String>,
    selected: usize,
    scroll: UniformListScrollHandle,
    error: Option<String>,
    _subscription: Subscription,
}

impl ThemePicker {
    fn filter(&mut self, query: &str) {
        let query = query.trim().to_lowercase();
        self.filtered = self
            .names
            .iter()
            .filter(|name| name.to_lowercase().contains(&query))
            .cloned()
            .collect();
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }
}

impl HerdrWindow {
    pub(super) fn open_theme_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu(window, cx);
        self.menu.page = Some(Page::Themes);
        let (names, error) = match self.config.available_themes() {
            Ok(names) => (names, None),
            Err(error) => (
                Theme::BUILTIN_NAMES
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
                Some(error),
            ),
        };
        let mut picker = if let Some(picker) = self.menu.themes.take() {
            picker.search.update(cx, |input, cx| input.clear(cx));
            picker
        } else {
            let search = cx.new(SearchInput::new);
            let subscription = cx.subscribe(
                &search,
                |this, search, _: &crate::search_input::Changed, cx| {
                    if let Some(picker) = &mut this.menu.themes {
                        picker.filter(search.read(cx).text());
                        cx.notify();
                    }
                },
            );
            ThemePicker {
                search,
                names: Vec::new(),
                filtered: Vec::new(),
                selected: 0,
                scroll: UniformListScrollHandle::new(),
                error: None,
                _subscription: subscription,
            }
        };
        picker.names = names;
        picker.error = error;
        picker.filter("");
        picker.search.update(cx, |input, cx| {
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            window.focus(&input.focus);
        });
        self.menu.themes = Some(picker);
        cx.notify();
    }

    fn apply_picker_theme(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.config.clone();
        config.theme = name.into();
        match config.theme().and_then(|theme| {
            config.save_theme(name)?;
            Ok(theme)
        }) {
            Ok(theme) => {
                self.config.theme = name.into();
                self.theme = theme;
                self.sync_terminal(cx);
                self.composer.update(cx, |editor, cx| {
                    editor.set_appearance(&self.config.ui, &self.theme, cx)
                });
                self.dismiss_menu(window, cx);
            }
            Err(error) => {
                if let Some(picker) = &mut self.menu.themes {
                    picker.error = Some(error);
                }
                cx.notify();
            }
        }
    }

    pub(super) fn theme_picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &mut self.menu.themes else {
            return;
        };
        // Unhandled text must reach the native input system, including IME commands.
        if picker.search.read(cx).is_composing() {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                cx.stop_propagation();
                window.prevent_default();
                self.dismiss_menu(window, cx);
            }
            "up" | "down" if !picker.filtered.is_empty() => {
                cx.stop_propagation();
                window.prevent_default();
                let count = picker.filtered.len();
                picker.selected = (picker.selected
                    + if event.keystroke.key == "up" {
                        count - 1
                    } else {
                        1
                    })
                    % count;
                picker
                    .scroll
                    .scroll_to_item(picker.selected, ScrollStrategy::Center);
                cx.notify();
            }
            "enter" => {
                cx.stop_propagation();
                window.prevent_default();
                if let Some(name) = picker.filtered.get(picker.selected).cloned() {
                    self.apply_picker_theme(&name, window, cx);
                }
            }
            _ => {}
        }
    }

    pub(super) fn render_theme_picker(&self, cx: &mut Context<Self>) -> Div {
        let Some(picker) = &self.menu.themes else {
            return div();
        };
        let theme = &self.theme;
        let font = &self.config.ui;
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(
                div()
                    .flex_none()
                    .p(px(16.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(font.size * 1.35))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Color Scheme"),
                            )
                            .child(
                                div()
                                    .id("theme-close")
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .rounded(px(4.))
                                    .hover(|s| s.bg(rgb(theme.active)))
                                    .child("Close")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.dismiss_menu(window, cx)
                                    })),
                            ),
                    )
                    .child(div().pt(px(12.)).child(picker.search.clone()))
                    .child(div().pt(px(8.)).text_color(rgb(theme.muted)).child(format!(
                        "{} of {} themes",
                        picker.filtered.len(),
                        picker.names.len()
                    ))),
            )
            .when_some(picker.error.clone(), |panel, error| {
                panel.child(
                    div()
                        .id("theme-error")
                        .max_h(px(90.))
                        .overflow_y_scroll()
                        .flex_none()
                        .p(px(12.))
                        .text_color(rgb(theme.foreground))
                        .bg(rgb(theme.active))
                        .child(error),
                )
            })
            .when(picker.filtered.is_empty(), |panel| {
                panel.child(
                    div()
                        .debug_selector(|| "theme-empty".into())
                        .flex_1()
                        .p(px(16.))
                        .text_color(rgb(theme.muted))
                        .child("No matching themes. Try a shorter search."),
                )
            })
            .when(!picker.filtered.is_empty(), |panel| {
                panel.child(
                    uniform_list(
                        "theme-results",
                        picker.filtered.len(),
                        cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                            let Some(picker) = &this.menu.themes else {
                                return Vec::new();
                            };
                            range
                                .map(|index| {
                                    let name = picker.filtered[index].clone();
                                    let selected = index == picker.selected;
                                    let current = name == this.config.theme;
                                    div()
                                        .id(index)
                                        .debug_selector(move || format!("theme-row-{index}"))
                                        .h(px(this.config.ui.line_height() + 20.))
                                        .px(px(16.))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .cursor_pointer()
                                        .when(selected, |row| row.bg(rgb(this.theme.active)))
                                        .hover(|s| s.bg(rgb(this.theme.active)))
                                        .child(
                                            div()
                                                .debug_selector(|| format!("theme-name-{name}"))
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .child(name.clone()),
                                        )
                                        .when(current, |row| {
                                            row.child(
                                                div()
                                                    .text_color(rgb(this.theme.muted))
                                                    .child("Current"),
                                            )
                                        })
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.apply_picker_theme(&name, window, cx)
                                        }))
                                })
                                .collect()
                        }),
                    )
                    .track_scroll(picker.scroll.clone())
                    .flex_1()
                    .min_h_0(),
                )
            })
            .child(
                div()
                    .flex_none()
                    .px(px(16.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_color(rgb(theme.muted))
                    .child(
                        "Up / Down to navigate. Enter or click to apply and save. Esc to cancel.",
                    ),
            )
    }
}
