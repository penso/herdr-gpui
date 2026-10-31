//! Drawing the grouped results that replace the spaces list while the search
//! holds a query.

use super::{Hit, Kind};
use crate::{
    HerdrWindow,
    config::{Theme, corners, mix},
    i18n::fl,
    sidebar::{
        agents::{Indicators, status_indicator},
        label_text,
        layout::SidebarLook,
        line_height,
    },
};
use gpui::{prelude::*, *};

impl HerdrWindow {
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
                    .child(label_text(&fl!("sidebar-search-no-matches"))),
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
                        .child(label_text(&match hit.kind {
                            Kind::Device => fl!("sidebar-search-devices"),
                            Kind::Worktree => fl!("sidebar-search-worktrees"),
                            Kind::Branch => fl!("sidebar-search-branches"),
                            Kind::Note => fl!("sidebar-search-notes"),
                        })),
                );
            }
            let lead = match (hit.kind, hit.status) {
                (Kind::Worktree, Some(status)) => {
                    status_indicator(status, font, indicators).into_any_element()
                }
                (kind, _) => svg()
                    .path(match kind {
                        Kind::Device => "icons/devices.svg",
                        Kind::Note => "icons/note.svg",
                        Kind::Worktree | Kind::Branch => "icons/git-branch.svg",
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
