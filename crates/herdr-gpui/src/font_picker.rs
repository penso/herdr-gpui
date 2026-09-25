//! Searchable installed-family picker; enumeration and config writes stay off the UI thread.
use crate::{
    HerdrWindow,
    config::{Config, FontFace},
    menu::Page,
    search_input::SearchInput,
};
use gpui::{prelude::*, *};

const DEFAULT_LABEL: &str = "Platform default";

fn font_names(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut names: Vec<_> = names
        .into_iter()
        .filter(|name| !name.trim().is_empty())
        .collect();
    names.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
    names.dedup();
    names
}

pub(crate) struct FontPicker {
    face: FontFace,
    search: Entity<SearchInput>,
    names: Vec<String>,
    filtered: Vec<Option<String>>,
    selected: usize,
    scroll: UniformListScrollHandle,
    loading: bool,
    error: Option<String>,
    _subscription: Subscription,
}

fn matching_names(names: &[String], query: &str) -> Vec<Option<String>> {
    let query = query.trim().to_lowercase();
    std::iter::once(None)
        .chain(names.iter().cloned().map(Some))
        .filter(|name| {
            name.as_deref()
                .unwrap_or(DEFAULT_LABEL)
                .to_lowercase()
                .contains(&query)
        })
        .collect()
}

impl FontPicker {
    fn filter(&mut self, query: &str) {
        self.filtered = matching_names(&self.names, query);
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }
}

impl HerdrWindow {
    pub(super) fn open_font_picker(
        &mut self,
        face: FontFace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.config_load.is_some() || !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::Fonts);
        let search = cx.new(SearchInput::new);
        search.update(cx, |input, cx| {
            input.set_placeholder("Search installed fonts...", cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            window.focus(&input.focus, cx);
        });
        let subscription = cx.subscribe(
            &search,
            |this, search, _: &crate::search_input::Changed, cx| {
                if let Some(picker) = &mut this.menu.fonts {
                    picker.filter(search.read(cx).text());
                    cx.notify();
                }
            },
        );
        let mut picker = FontPicker {
            face,
            search,
            names: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            loading: true,
            error: None,
            _subscription: subscription,
        };
        picker.filter("");
        self.menu.fonts = Some(picker);
        let text_system = cx.text_system().clone();
        let names = cx
            .background_executor()
            .spawn(async move { font_names(text_system.all_font_names()) });
        cx.spawn(async move |this, cx| {
            let names = names.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(picker) = &mut this.menu.fonts
                    && picker.face == face
                    && this.menu.page == Some(Page::Fonts)
                {
                    picker.names = names;
                    picker.loading = false;
                    picker.filter(picker.search.read(cx).text());
                    let current = match face {
                        FontFace::Sidebar => &this.config.sidebar.family,
                        FontFace::Tabs => &this.config.tabs.family,
                        FontFace::Terminal => &this.config.terminal.family,
                        FontFace::Ui => &this.config.ui.family,
                    };
                    if picker.search.read(cx).text().is_empty() {
                        picker.selected = picker
                            .filtered
                            .iter()
                            .position(|name| name.as_deref() == Some(current))
                            .unwrap_or(0);
                        picker
                            .scroll
                            .scroll_to_item(picker.selected, ScrollStrategy::Center);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn choose_font_family(
        &mut self,
        family: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &self.menu.fonts else {
            return;
        };
        if picker.loading
            || self.config_load.is_some()
            || (family
                .as_ref()
                .is_some_and(|name| !picker.names.contains(name)))
        {
            return;
        }
        let face = picker.face;
        let text_system = cx.text_system().clone();
        self.load_gui_config_with(
            move || {
                Config::save_font_family(face, family.as_deref())?;
                let mut config = Config::load()?;
                config.resolve_font_fallbacks(|| text_system.all_font_names());
                let theme = config.theme()?;
                Ok((config, theme))
            },
            cx,
        );
        self.menu.page = Some(Page::Preferences);
        self.menu.fonts = None;
        window.focus(&self.menu.focus, cx);
        cx.notify();
    }

    pub(super) fn font_picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &mut self.menu.fonts else {
            return;
        };
        if picker.search.read(cx).is_composing() {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                cx.stop_propagation();
                window.prevent_default();
                self.menu.page = Some(Page::Preferences);
                self.menu.fonts = None;
                window.focus(&self.menu.focus, cx);
                cx.notify();
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
                if let Some(family) = picker.filtered.get(picker.selected).cloned() {
                    self.choose_font_family(family, window, cx);
                }
            }
            _ => {}
        }
    }

    pub(super) fn render_font_picker(&self, cx: &mut Context<Self>) -> Div {
        let Some(picker) = &self.menu.fonts else {
            return div();
        };
        let theme = &self.theme;
        let font = &self.config.ui;
        div().flex().flex_col().size_full().min_h_0()
            .child(div().flex_none().p(px(16.)).border_b_1().border_color(rgb(theme.active))
                .child(div().flex().items_center()
                    .child(div().flex_1().text_size(px(font.size * 1.35)).font_weight(FontWeight::SEMIBOLD).child(format!("{} Font", picker.face.name())))
                    .child(div().id("font-picker-back").cursor_pointer().child("Back").on_click(cx.listener(|this, _, window, cx| {
                        this.menu.page = Some(Page::Preferences); this.menu.fonts = None; window.focus(&this.menu.focus, cx); cx.notify();
                    }))))
                .child(div().pt(px(12.)).child(picker.search.clone()))
                .child(div().pt(px(8.)).text_color(rgb(theme.muted)).child(format!("{} of {} installed fonts", picker.filtered.iter().filter(|name| name.is_some()).count(), picker.names.len()))))
            .when(picker.filtered.is_empty() || picker.loading, |panel| panel.child(div().flex_1().p(px(16.)).child(if picker.loading { "Loading installed fonts..." } else { "No matching fonts." })))
            .when(!picker.filtered.is_empty() && !picker.loading, |panel| panel.child(
                uniform_list("font-results", picker.filtered.len(), cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                    let Some(picker) = &this.menu.fonts else { return Vec::new(); };
                    range.map(|index| {
                        let family = picker.filtered[index].clone();
                        let label = family.clone().unwrap_or_else(|| DEFAULT_LABEL.into());
                        div().id(index).debug_selector(move || format!("font-row-{index}"))
                            .w_full().h(px(this.config.ui.line_height() + 20.)).px(px(16.))
                            .flex().items_center().cursor_pointer()
                            .when(index == picker.selected, |row| row.bg(rgb(this.theme.active)))
                            .hover(|s| s.bg(rgb(this.theme.active)))
                            .child(div().flex_1().min_w_0().truncate().child(label))
                            .on_click(cx.listener(move |this, _, window, cx| this.choose_font_family(family.clone(), window, cx)))
                    }).collect()
                })).track_scroll(&picker.scroll).flex_1().min_h_0()))
            .child(div().flex_none().p(px(12.)).border_t_1().border_color(rgb(theme.active)).text_color(rgb(theme.muted))
                .child(picker.error.clone().unwrap_or_else(|| "Type to filter; Enter or click to save. Platform default clears the override.".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    #[test]
    fn installed_fonts_are_sorted_deduplicated_and_not_filtered_by_face() {
        assert_eq!(
            font_names(["Zed", "Alpha", "Zed", "Mono", " "].map(str::to_owned)),
            vec!["Alpha", "Mono", "Zed"]
        );
    }

    #[test]
    fn filtering_keeps_selection_and_reset_distinct() {
        let names = font_names(["Mono", "Sans"].map(str::to_owned));
        assert_eq!(matching_names(&names, "mon"), vec![Some("Mono".into())]);
        assert_eq!(matching_names(&names, "platform"), vec![None]);
        assert_eq!(
            matching_names(&names, ""),
            vec![None, Some("Mono".into()), Some("Sans".into())]
        );
    }
}
