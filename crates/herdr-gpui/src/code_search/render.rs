//! The picker paints ranked rows; ranking and input live elsewhere.

use super::{CodeSearch, Mode, Status, rank};
use crate::{
    HerdrWindow,
    code_index::{Change, Kind},
    config::Theme,
    i18n::fl,
};
use gpui::{prelude::*, *};
use std::time::SystemTime;

/// What a changed file's row says under its path: whether Git tracks it,
/// and how long ago it was last written.
pub(super) fn changed_detail(change: &Change) -> String {
    let ago = change
        .modified
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map(|elapsed| crate::ago::text(elapsed.as_secs()));
    match (change.new, ago) {
        (true, Some(ago)) => fl!("code-search-new-ago", ago = ago),
        (false, Some(ago)) => fl!("code-search-changed-ago", ago = ago),
        (true, None) => fl!("code-search-new"),
        (false, None) => fl!("code-search-changed"),
    }
}

/// What kind of definition a symbol is, from the index's typed kind.
fn kind_label(kind: Kind) -> String {
    match kind {
        Kind::Function => fl!("code-search-kind-function"),
        Kind::Method => fl!("code-search-kind-method"),
        Kind::Type => fl!("code-search-kind-type"),
        Kind::Module => fl!("code-search-kind-module"),
        Kind::Constant => fl!("code-search-kind-constant"),
        Kind::Implementation => fl!("code-search-kind-implementation"),
        Kind::Macro => fl!("code-search-kind-macro"),
    }
}

/// A changed file's line counts, or `new` for a file Git does not track.
fn change_badge(theme: &Theme, change: &Change) -> Div {
    let badge = div().flex().gap(px(6.));
    match change.counts {
        _ if change.new => badge
            .text_color(rgb(crate::menu::online(theme)))
            .child(fl!("code-search-badge-new")),
        Some((added, removed)) => badge
            .child(
                div()
                    .text_color(rgb(crate::menu::online(theme)))
                    .child(format!("+{added}")),
            )
            .child(
                div()
                    .text_color(crate::menu::danger(theme))
                    .child(format!("\u{2212}{removed}")),
            ),
        None => badge
            .text_color(rgb(theme.muted))
            .child(fl!("code-search-badge-binary")),
    }
}

impl HerdrWindow {
    fn code_search_header(&self, code: &CodeSearch, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let counted = code.index.as_ref().map(|index| match code.mode {
            Mode::Symbols => index.symbols().len(),
            Mode::Files => index.files().len(),
        });
        let summary = match (&code.status, counted) {
            (Status::Failed(_), _) => String::new(),
            (Status::Locating, _) | (Status::Indexing, None) => fl!("code-search-reading"),
            (_, Some(count)) => {
                let shown = code.hits.len();
                let more = if shown == rank::MAX_HITS { "yes" } else { "no" };
                let partial = code.index.as_ref().is_some_and(|index| index.truncated());
                let partial = if partial { "yes" } else { "no" };
                fl!(
                    "code-search-count",
                    shown = shown,
                    more = more,
                    total = count,
                    partial = partial
                )
            }
            (Status::Ready, None) => String::new(),
        };
        let root = code
            .index
            .as_ref()
            .map(|index| index.root().display().to_string())
            .unwrap_or_default();
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
                            .flex_none()
                            .text_size(px(self.config.ui.size * 1.35))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(match code.mode {
                                Mode::Symbols => fl!("code-search-title-symbols"),
                                Mode::Files => fl!("code-search-title-files"),
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(theme.muted))
                            .child(root),
                    ),
            )
            .child(div().pt(px(12.)).child(code.search.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .pt(px(10.))
                    .children(Mode::ALL.into_iter().map(|mode| {
                        div()
                            .id(("code-search-mode", mode as usize))
                            .debug_selector(move || format!("code-search-mode-{}", mode.key()))
                            .px_2()
                            .py_1()
                            .rounded(px(crate::config::corners::CONTROL))
                            .cursor_pointer()
                            .when(mode == code.mode, |tab| tab.bg(rgb(theme.active)))
                            .hover(|tab| tab.bg(rgb(theme.active)))
                            .child(mode.label())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_code_search_mode(mode, window, cx);
                            }))
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_right()
                            .text_color(rgb(theme.muted))
                            .child(summary),
                    ),
            )
    }

    fn code_search_row(
        &self,
        code: &CodeSearch,
        row: usize,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let hit = code.hits.get(row)?;
        let index = code.index.as_ref()?;
        let label: SharedString = rank::label(index, code.mode, hit.item).to_owned().into();
        let theme = &self.theme;
        let (detail, badge) = match code.mode {
            Mode::Symbols => {
                let symbol = index.symbols().get(hit.item)?;
                let file = index.files().get(symbol.file)?;
                let place = format!("{file}:{}", symbol.line);
                let detail = match index.change(symbol.file) {
                    Some(_) => fl!("code-search-symbol-changed", place = place),
                    None => place,
                };
                let badge = div()
                    .text_color(rgb(theme.muted))
                    .child(kind_label(symbol.kind));
                (detail, badge)
            }
            Mode::Files => match index.change(hit.item) {
                Some(change) => (changed_detail(change), change_badge(theme, change)),
                None => (String::new(), div()),
            },
        };
        let matched = HighlightStyle {
            color: Some(rgb(self.theme.primary()).into()),
            font_weight: Some(FontWeight::BOLD),
            ..Default::default()
        };
        let label = StyledText::new(label)
            .with_highlights(hit.highlights.iter().map(|range| (range.clone(), matched)));
        Some(
            div()
                .id(row)
                .debug_selector(move || format!("code-search-row-{row}"))
                .w_full()
                .h(px(self.config.ui.line_height() * 2. + 12.))
                .px(px(16.))
                .flex()
                .items_center()
                .gap(px(12.))
                .cursor_pointer()
                .when(row == code.selected, |div| div.bg(rgb(self.theme.active)))
                .hover(|s| s.bg(rgb(self.theme.active)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().truncate().child(label))
                        .when(!detail.is_empty(), |column| {
                            column.child(
                                div()
                                    .truncate()
                                    .text_color(rgb(self.theme.muted))
                                    .child(detail),
                            )
                        }),
                )
                .child(badge.flex_none())
                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    let viewer = event.modifiers().secondary();
                    this.activate_code_search(row, viewer, window, cx);
                })),
        )
    }

    pub(crate) fn render_code_search(&self, cx: &mut Context<Self>) -> Div {
        let Some(code) = &self.menu.code_search else {
            return div();
        };
        let theme = &self.theme;
        let empty = match &code.status {
            Status::Failed(error) => Some(error.clone()),
            Status::Locating | Status::Indexing if code.index.is_none() => None,
            _ if code.hits.is_empty() && code.rank_task.is_none() => Some(match code.mode {
                Mode::Symbols => fl!("code-search-no-symbols"),
                Mode::Files => fl!("code-search-no-files"),
            }),
            _ => None,
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(self.code_search_header(code, cx))
            .when_some(empty, |panel, text| {
                panel.child(
                    div()
                        .debug_selector(|| "code-search-empty".into())
                        .flex_1()
                        .p(px(16.))
                        .text_color(rgb(theme.muted))
                        .child(text),
                )
            })
            .when(!code.hits.is_empty(), |panel| {
                panel.child(
                    uniform_list(
                        "code-search-results",
                        code.hits.len(),
                        cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                            let Some(code) = &this.menu.code_search else {
                                return Vec::new();
                            };
                            range
                                .filter_map(|row| this.code_search_row(code, row, cx))
                                .collect()
                        }),
                    )
                    .track_scroll(&code.scroll)
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
                    .child(fl!(
                        "code-search-hints",
                        viewer = if cfg!(target_os = "macos") {
                            "⌘Enter" // i18n: data
                        } else {
                            "Ctrl-Enter" // i18n: data
                        }
                    )),
            )
    }
}
