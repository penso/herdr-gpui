//! The theme search box, result count and card grid on the Theme page.
use super::{ROW_HEIGHT, Scope, SettingsWindow, grid};
use crate::{
    config::{Theme, ThemeName, corners},
    herdr_settings,
    i18n::fl,
    settings_window::ui,
};
use gpui::{Context, Div, ScrollStrategy, Window, div, prelude::*, px, rgb, uniform_list};

impl SettingsWindow {
    pub(in crate::settings_window) fn render_theme_search(&self, cx: &mut Context<Self>) -> Div {
        div()
            .debug_selector(|| "settings-theme-search".into())
            .h(px(32.))
            .flex_none()
            .on_key_down(cx.listener(Self::theme_browser_key))
            .child(self.themes.search.clone())
    }

    pub(in crate::settings_window) fn render_theme_results(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        #[cfg(all(feature = "integration-test", target_os = "macos"))]
        grid::clear_native_bounds(cx);
        let columns = grid::columns(f32::from(window.viewport_size().width) - 240.);
        if self.themes.grid.columns != columns {
            self.themes.grid.columns = columns;
            self.themes.scroll.scroll_to_item(
                self.themes.selected.unwrap_or(0) / columns,
                ScrollStrategy::Center,
            );
        }
        let browser = &self.themes;
        let theme = &self.theme;
        let count = usize::from(browser.ghostty_enabled) * browser.names.len()
            + usize::from(browser.herdr_enabled) * herdr_settings::THEME_NAMES.len();
        let button = |id: &'static str, label: String| ui::button(theme, id, label, true);
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .debug_selector(|| "settings-theme-count".into())
                    .text_size(px(11.))
                    .text_color(rgb(theme.subtext()))
                    .child(if browser.discovering {
                        fl!(
                            "settings-theme-count-discovering",
                            shown = browser.filtered.len(),
                            total = count,
                        )
                    } else {
                        fl!(
                            "settings-theme-count",
                            shown = browser.filtered.len(),
                            total = count,
                        )
                    }),
            )
            .child(
                div()
                    .id("settings-theme-list")
                    .debug_selector(|| "settings-theme-list".into())
                    // The child list scrolls first; keep wheel events inside the
                    // grid even at its edges or when the results are empty.
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .flex_1()
                    .min_h(px(240.))
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if !*hovered {
                            this.hover_theme(None, cx);
                        }
                    }))
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(theme.active))
                    .rounded(px(corners::CONTROL))
                    .when(browser.filtered.is_empty(), |el| {
                        el.child(
                            div()
                                .debug_selector(|| "settings-theme-empty".into())
                                .p(px(12.))
                                .child(fl!("settings-theme-empty")),
                        )
                    })
                    .when(!browser.filtered.is_empty(), |el| {
                        el.child(
                            uniform_list(
                                "settings-theme-results",
                                browser.filtered.len().div_ceil(columns),
                                cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                                    this.schedule_grid_palettes(range.clone(), cx);
                                    range
                                        .map(|row| div().h(px(ROW_HEIGHT)).w_full().flex().gap(px(6.)).children((0..this.themes.grid.columns).map(|column| {
                                            let index = row * this.themes.grid.columns + column;
                                            if index >= this.themes.filtered.len() {
                                                return div().flex_1().min_w_0().into_any_element();
                                            }
                                             let choice = this.themes.filtered[index].clone();
                                             let name = &choice.name;
                                            let draft =
                                                this.theme_intent.as_ref().is_some_and(|intent| {
                                                    intent.choice.scope == choice.scope
                                                        && match choice.scope {
                                                            Scope::App => {
                                                                ThemeName::side(&intent.choice.name, this.themes.editing_light) == name
                                                            }
                                                            Scope::Herdr => intent.choice.name == *name,
                                                        }
                                                });
                                             let saved = match choice.scope {
                                                 Scope::App => name == this.edited_theme(),
                                                Scope::Herdr => {
                                                    this.shared.as_ref().is_some_and(|shared| {
                                                         shared.theme_name == *name
                                                    })
                                                }
                                            };
                                             let source = match choice.scope {
                                                Scope::Herdr => fl!("settings-theme-kind-herdr"),
                                                Scope::App
                                                    if Theme::BUILTIN_NAMES
                                                        .contains(&name.as_str()) =>
                                                {
                                                    fl!("settings-theme-kind-built-in")
                                                }
                                                Scope::App
                                                     if std::path::Path::new(name)
                                                        .is_absolute()
                                                        || name.starts_with("~/") =>
                                                {
                                                    fl!("settings-theme-kind-file")
                                                }
                                                Scope::App => fl!("settings-theme-kind-ghostty"),
                                            };
                                            div()
                                                .id(index)
                                                .relative()
                                                .map(|card| {
                                                    #[cfg(all(feature = "integration-test", target_os = "macos"))]
                                                    let card = card.child(grid::native_probe(index));
                                                    card
                                                })
                                                .debug_selector(move || {
                                                    format!("settings-theme-row-{index}")
                                                })
                                                .h(px(76.))
                                                .flex_1()
                                                .min_w_0()
                                                .overflow_hidden()
                                                .px(px(7.))
                                                .py(px(4.))
                                                .flex()
                                                .flex_col()
                                                .rounded(px(corners::CONTROL))
                                                .border_2()
                                                .border_color(rgb(if this.themes.selected == Some(index) { this.theme.primary() } else { this.theme.active }))
                                                 .when(choice.scope.editable(), |el| el.cursor_pointer())
                                                .hover(|el| el.bg(rgb(this.theme.active)))
                                                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                                                    // Leaving is handled by the grid, so moving between
                                                    // cards never flashes the saved theme.
                                                    if *hovered {
                                                        this.hover_theme(Some(index), cx);
                                                    }
                                                }))
                                                 .child(this.grid_thumbnail(&choice, index))
                                                .child(
                                                    div()
                                                        .debug_selector(move || {
                                                            format!("settings-theme-name-{index}")
                                                        })
                                                        .min_w_0()
                                                        .truncate()
                                                         .child(name.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .debug_selector(move || {
                                                            format!("settings-theme-source-{index}")
                                                        })
                                                        .text_size(px(10.))
                                                        .text_color(rgb(this.theme.subtext()))
                                                        .truncate()
                                                        .child(if draft { fl!("settings-theme-card-draft", source = source) } else if saved { fl!("settings-theme-card-saved", source = source) } else { source }),
                                                )
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        let focus = this
                                                            .themes
                                                            .search
                                                            .read(cx)
                                                            .focus
                                                            .clone();
                                                        window.focus(&focus, cx);
                                                        this.select_settings_theme(index, cx);
                                                    },
                                                )).into_any_element()
                                        })))
                                        .collect()
                                }),
                            )
                            .track_scroll(&browser.scroll)
                            .h_full()
                            .w_full(),
                        )
                    }),
            )
            .when_some(browser.catalog_error.as_ref(), |el, error| {
                el.child(
                    div()
                        .debug_selector(|| "settings-theme-catalog-error".into())
                        .child(fl!("settings-theme-catalog-error", error = error.as_str()))
                        .child(
                            button("theme-retry-catalog", fl!("settings-theme-retry-catalog"))
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.discover_settings_themes(cx)),
                                ),
                        ),
                )
            })
            .when_some(browser.preview_error.as_ref(), |el, error| {
                el.child(
                    div()
                        .debug_selector(|| "settings-theme-preview-error".into())
                        .child(fl!("settings-theme-preview-error", error = error.as_str()))
                        .child(
                            button("theme-retry-preview", fl!("settings-theme-retry-preview")).on_click(
                                cx.listener(|this, _, _, cx| this.request_theme_preview(cx)),
                            ),
                        ),
                )
            })
    }
}
