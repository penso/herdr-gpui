//! Searching the Settings: the query in the section list's box swaps the body
//! for a list of matching rows, and picking one opens its page at that row.
use super::{SettingsWindow, catalog, ui};
use crate::config::ThemeName;
use gpui::{prelude::*, *};
use std::collections::HashMap;

/// Where each row was last painted, by row id, for scrolling to it.
#[derive(Default)]
pub(super) struct RowBounds(pub HashMap<SharedString, Bounds<Pixels>>);
impl Global for RowBounds {}

/// Records the bounds of the element it is a child of under `id`. Absolute, so
/// it takes no room in the row's layout.
pub(super) fn track_bounds(id: SharedString) -> impl IntoElement {
    canvas(
        move |bounds, _, cx| {
            cx.default_global::<RowBounds>().0.insert(id, bounds);
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// Space kept above a row that search scrolls to the top of the body.
const SCROLL_MARGIN: f32 = 12.;

impl SettingsWindow {
    /// Shows the results for `query`, or the page again when it is empty.
    pub(super) fn set_search_query(&mut self, query: &str, cx: &mut Context<Self>) {
        let results = (!query.trim().is_empty()).then(|| catalog::search(query));
        if results != self.search_results {
            if results.is_some() {
                // The results replace the page and any slider being dragged.
                self.drop_number_preview(cx);
            }
            self.search_results = results;
            self.body_scroll.set_offset(Point::default());
            cx.notify();
        }
    }

    /// Whether the page renders a row needing `availability` right now.
    pub(super) fn available(&self, availability: catalog::Availability) -> bool {
        match availability {
            catalog::Availability::Always => true,
            catalog::Availability::FollowingSystemTheme => {
                ThemeName::follows_system(self.drafted_theme())
            }
            catalog::Availability::Windows => cfg!(windows),
        }
    }

    /// The current results without rows the pages do not render now.
    pub(super) fn visible_results(&self) -> Vec<usize> {
        self.search_results
            .iter()
            .flatten()
            .copied()
            .filter(|index| {
                catalog::CATALOG
                    .get(*index)
                    .is_some_and(|entry| self.available(entry.availability))
            })
            .collect()
    }

    /// Drops the query and its results, leaving the selected page showing.
    pub(super) fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.search_results = None;
        self.nav_search.update(cx, |input, cx| input.clear(cx));
    }

    /// Enter opens the first result and Escape clears the query, while the
    /// search box has focus. Returns whether the key was used.
    pub(super) fn search_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let input = self.nav_search.read(cx);
        if self.search_results.is_none() || input.is_composing() || !input.focus.is_focused(window)
        {
            return false;
        }
        match key {
            "enter" => {
                if let Some(index) = self.visible_results().first().copied() {
                    self.open_result(index, window, cx);
                }
            }
            "escape" => self.clear_search(cx),
            _ => return false,
        }
        window.prevent_default();
        true
    }

    /// Opens the page of catalog entry `index` and scrolls its row into view.
    pub(super) fn open_result(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = catalog::CATALOG.get(index) else {
            return;
        };
        cx.default_global::<RowBounds>().0.clear();
        self.select_section(entry.section, window, cx);
        let id = entry.id;
        // The page paints in the next frame; its rows report their bounds then.
        cx.on_next_frame(window, move |_, window, cx| {
            cx.on_next_frame(window, move |this, _, cx| this.scroll_to_row(id, cx));
        });
    }

    /// Scrolls the body so the row `id` sits at its top, if it was painted.
    pub(super) fn scroll_to_row(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(row) = cx.default_global::<RowBounds>().0.get(id).copied() else {
            return;
        };
        let offset = self.body_scroll.offset();
        let top = self.body_scroll.bounds().top();
        let wanted = (row.top() - top - px(SCROLL_MARGIN)).max(px(0.));
        let furthest = self.body_scroll.max_offset().y;
        let y = (offset.y - wanted).max(-furthest).min(px(0.));
        self.body_scroll.set_offset(point(offset.x, y));
        cx.notify();
    }

    /// The body while a query is typed: one row per match, in catalog order.
    pub(super) fn render_search_results(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let results = self.visible_results();
        if results.is_empty() {
            return div()
                .debug_selector(|| "settings-search-empty".into())
                .text_color(rgb(theme.subtext()))
                .child("No settings match.");
        }
        div().flex().flex_col().children(
            results
                .iter()
                .copied()
                .filter_map(|index| catalog::CATALOG.get(index).map(|entry| (index, entry)))
                .enumerate()
                .map(|(n, (index, entry))| {
                    let id = format!("settings-search-result-{n}");
                    let mut row = ui::row(
                        theme,
                        id.clone(),
                        format!("{} \u{203a} {}", entry.section.label(), entry.title),
                    );
                    if !entry.description.is_empty() {
                        row = row.description(entry.description);
                    }
                    row.control(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(theme.subtext()))
                            .child(entry.section.label()),
                    )
                    .into_element()
                    .id(ElementId::Name(id.into()))
                    .cursor_pointer()
                    .hover(|row| row.bg(rgb(theme.active)))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.open_result(index, window, cx)),
                    )
                }),
        )
    }
}
