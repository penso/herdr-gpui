//! Drawing the search field and the grouped results that replace the spaces
//! list while it holds a query.

use super::{Hit, Kind};
use crate::{
    HerdrWindow,
    config::{Theme, corners, mix},
    sidebar::{
        agents::{Indicators, status_indicator},
        label_text,
        layout::SidebarLook,
        line_height,
    },
};
use gpui::{prelude::*, *};

impl HerdrWindow {
    /// The field, with a clear button once it holds text.
    pub(in crate::sidebar) fn sidebar_search_field(
        &self,
        look: SidebarLook,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let typed = self.sidebar_search.query().is_some();
        div()
            .debug_selector(|| "sidebar-search".into())
            .flex_none()
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(look.content_x()))
            .pb(px(6.))
            .on_key_down(cx.listener(Self::sidebar_search_key))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(self.sidebar_search.input.clone()),
            )
            .when(typed, |field| {
                field.child(
                    div()
                        .id("sidebar-search-clear")
                        .debug_selector(|| "sidebar-search-clear".into())
                        .flex_none()
                        .size(px(20.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(corners::SMALL))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(theme.active)))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.clear_sidebar_search(window, cx);
                        }))
                        .child(
                            svg()
                                .path("icons/x.svg")
                                .size(px(12.))
                                .text_color(rgb(theme.muted)),
                        ),
                )
            })
    }

    /// The results for `hits`, grouped under a heading per kind.
    pub(in crate::sidebar) fn sidebar_search_results(
        &self,
        hits: Vec<Hit>,
        indicators: Indicators,
        look: SidebarLook,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let font = &self.config.sidebar;
        let content_x = look.content_x();
        let mut list = div()
            .id("sidebar-search-results")
            .debug_selector(|| "sidebar-search-results".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.sidebar_search.scroll);
        if hits.is_empty() {
            return list.child(
                div()
                    .px(px(content_x))
                    .text_color(rgb(theme.muted))
                    .truncate()
                    .child(label_text("no matches")),
            );
        }
        let selected = self.sidebar_search.selected.min(hits.len() - 1);
        let mut section = None;
        for (index, hit) in hits.into_iter().enumerate() {
            if section != Some(hit.kind) {
                section = Some(hit.kind);
                list = list.child(
                    div()
                        .px(px(content_x))
                        .pt(px(if index == 0 { 0. } else { 8. }))
                        .pb(px(2.))
                        .text_size(px((font.size * 0.85).round()))
                        .text_color(rgb(theme.muted))
                        .child(label_text(match hit.kind {
                            Kind::Device => "devices",
                            Kind::Worktree => "worktrees",
                            Kind::Branch => "branches",
                        })),
                );
            }
            let lead = match (hit.kind, hit.status) {
                (Kind::Worktree, Some(status)) => {
                    status_indicator(status, font, indicators).into_any_element()
                }
                (kind, _) => svg()
                    .path(if kind == Kind::Device {
                        "icons/devices.svg"
                    } else {
                        "icons/git-branch.svg"
                    })
                    .size(px(indicators.width(font).max(12.)))
                    .flex_none()
                    .text_color(rgb(theme.subtext()))
                    .into_any_element(),
            };
            let target = hit.target.clone();
            let hover = mix(theme.sidebar_background(), theme.active, 60);
            list = list.child(
                div()
                    .id(("sidebar-search-hit", index))
                    .debug_selector(move || format!("sidebar-search-hit-{index}"))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(look.density.gap()))
                    .mx(px(4.))
                    .px(px((content_x - 4.).max(0.)))
                    .py(px(2.))
                    .rounded(px(corners::SMALL))
                    .cursor_pointer()
                    .when(index == selected, |row| row.bg(rgb(theme.active)))
                    .when(index != selected, |row| {
                        row.hover(move |style| style.bg(rgb(hover)))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_search_hit(&target, window, cx);
                    }))
                    .child(lead)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .h(px(line_height(font)))
                                    .truncate()
                                    .child(marked(&hit, theme)),
                            )
                            .child(
                                div()
                                    .h(px(line_height(font)))
                                    .truncate()
                                    .text_color(rgb(theme.muted))
                                    .child(label_text(&hit.context)),
                            ),
                    ),
            );
        }
        list
    }
}

/// The result's text with its matching part picked out in the accent wash.
fn marked(hit: &Hit, theme: &Theme) -> StyledText {
    let wash = theme.primary_wash();
    StyledText::new(hit.text.clone()).with_highlights([(
        hit.range.clone(),
        HighlightStyle {
            color: Some(rgb(theme.text_on(wash)).into()),
            background_color: Some(rgb(wash).into()),
            font_weight: Some(FontWeight::BOLD),
            ..Default::default()
        },
    )])
}
