//! The palette paints prepared entries; input and daemon operations live elsewhere.

use super::{Action, AgentFilter, Filter, Palette, preview};
use crate::direction::Logical;
use crate::{HerdrWindow, i18n::fl};
use gpui::{prelude::*, *};

/// The narrowest window, less its margins, that fits Go To's list and
/// preview side by side; a narrower one shows the list alone.
const PREVIEW_MIN_WIDTH: f32 = 840.;

/// How much larger than the interface text the palette's query is drawn.
const SEARCH_SCALE: f32 = 1.5;

/// The interface font, enlarged for the palette's query field.
pub(super) fn search_font(ui: &crate::config::FontConfig) -> crate::config::FontConfig {
    crate::config::FontConfig {
        size: ui.size * SEARCH_SCALE,
        ..ui.clone()
    }
}

impl HerdrWindow {
    /// The query is the palette's focus, as in Raycast: a large frameless
    /// field spans the top, with the filters and count quieter beneath it.
    fn palette_header(&self, palette: &Palette, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        div()
            .flex_none()
            .child(
                div()
                    .px(px(20.))
                    .py(px(16.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(palette.search.clone()),
            )
            .child(
                div()
                    .px(px(14.))
                    .pt(px(8.))
                    .pb(px(6.))
                    .row()
                    .items_center()
                    .gap(px(12.))
                    .text_color(rgb(theme.muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .row()
                            .flex_wrap()
                            .gap(px(4.))
                            .children(Filter::ALL.into_iter().map(|filter| {
                                div()
                                    .id(("palette-filter", filter as usize))
                                    .debug_selector(move || {
                                        format!("palette-filter-{}", filter.name())
                                    })
                                    .px_2()
                                    .py_1()
                                    .rounded(px(crate::config::corners::CONTROL))
                                    .cursor_pointer()
                                    .when(filter == palette.filter, |tab| {
                                        tab.bg(rgb(theme.active)).text_color(rgb(theme.foreground))
                                    })
                                    .hover(|tab| tab.bg(rgb(theme.active)))
                                    .child(filter.label())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.set_palette_filter(filter, cx);
                                        if let Some(palette) = &this.menu.palette {
                                            let focus = palette.search.read(cx).focus.clone();
                                            window.focus(&focus, cx);
                                        }
                                    }))
                            })),
                    )
                    .child(div().flex_none().child(fl!(
                        "palette-count",
                        shown = palette.filtered.len(),
                        total = palette.entries.len(),
                    ))),
            )
            .when(palette.filter == Filter::Navigation, |header| {
                header.child(
                    div()
                        .px(px(14.))
                        .child(self.palette_agent_filters(palette, cx)),
                )
            })
    }

    /// Go To's status chips; Alt with the chip's navigator letter picks one
    /// from the keyboard.
    fn palette_agent_filters(&self, palette: &Palette, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        div()
            .row()
            .flex_wrap()
            .gap(px(6.))
            .pt(px(6.))
            .text_color(rgb(theme.muted))
            .children(AgentFilter::ALL.into_iter().map(|filter| {
                div()
                    .id(("palette-agent-filter", filter as usize))
                    .debug_selector(move || format!("palette-agent-filter-{}", filter.name()))
                    .px_2()
                    .py_1()
                    .rounded(px(crate::config::corners::CONTROL))
                    .cursor_pointer()
                    .when(filter == palette.agent_filter, |chip| {
                        chip.bg(rgb(theme.active)).text_color(rgb(theme.foreground))
                    })
                    .hover(|chip| chip.bg(rgb(theme.active)))
                    .child(filter.label())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_palette_agent_filter(filter, cx);
                        if let Some(palette) = &this.menu.palette {
                            let focus = palette.search.read(cx).focus.clone();
                            window.focus(&focus, cx);
                        }
                    }))
            }))
    }

    fn palette_row(
        &self,
        palette: &Palette,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let hit = &palette.filtered[index];
        let entry = &palette.entries[hit.index];
        // Ranked results need not keep entry order, so only an empty query nests.
        let depth = if palette.query.trim().is_empty() {
            hit.depth
        } else {
            0
        };
        let action = entry.action.clone();
        // Matched characters were found while ranking; painting only styles them.
        let matched = HighlightStyle {
            color: Some(rgb(self.theme.primary()).into()),
            font_weight: Some(FontWeight::BOLD),
            ..Default::default()
        };
        let styled = |text: &SharedString, ranges: &[std::ops::Range<usize>]| {
            StyledText::new(text.clone())
                .with_highlights(ranges.iter().map(|range| (range.clone(), matched)))
        };
        div()
            .id(index)
            .debug_selector(move || format!("palette-row-{index}"))
            .w_full()
            .h(px(self.config.ui.line_height() * 2. + 20.))
            .px(px(16.))
            .row()
            .items_center()
            .gap(px(12.))
            .cursor_pointer()
            .when(index == palette.selected, |row| {
                row.bg(rgb(self.theme.active))
            })
            .hover(|s| s.bg(rgb(self.theme.active)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .ps(px(16. * depth as f32))
                    .child(
                        div()
                            .truncate()
                            .child(styled(&entry.label, &hit.highlights.label)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_color(rgb(self.theme.muted))
                            .child(styled(&entry.detail, &hit.highlights.detail)),
                    ),
            )
            .when(!entry.badge.is_empty(), |row| {
                row.child(
                    div()
                        .flex_none()
                        .text_color(rgb(self.theme.muted))
                        .child(entry.badge.clone()),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.activate_palette(action.clone(), window, cx)
            }))
    }

    /// Whether Go To shows its preview: it is toggled on, and the window has
    /// room for it beside the list.
    pub(crate) fn palette_previews(&self, viewport: Size<Pixels>) -> bool {
        self.menu.palette_preview
            && self
                .menu
                .palette
                .as_ref()
                .is_some_and(|palette| palette.filter == Filter::Navigation)
            && f32::from(viewport.width) - 32. >= PREVIEW_MIN_WIDTH
    }

    /// The preview of the selected Go To row, from what its host's
    /// connection already holds.
    pub(super) fn palette_preview(&self, palette: &Palette) -> Option<preview::Preview> {
        let Action::Go {
            endpoint,
            boot,
            target,
        } = &palette.selected_entry()?.action
        else {
            return None;
        };
        let index = self.endpoints.iter().position(|e| e.id == *endpoint)?;
        let live = if index == self.selected_endpoint {
            &self.live
        } else {
            &self.endpoints[index].live
        };
        preview::preview(
            live.snapshot.as_deref(),
            live.surface.as_ref(),
            &self.endpoints[index].last_seen,
            boot,
            target.as_deref(),
        )
    }

    fn render_palette_preview(&self, palette: &Palette) -> Div {
        let theme = &self.theme;
        let pane = div()
            .debug_selector(|| "palette-preview".into())
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .border_s(px(1.))
            .border_color(rgb(theme.active));
        match self.palette_preview(palette) {
            Some(preview::Preview::Screen {
                surface,
                rect,
                seen_at,
            }) => {
                let terminal = &self.config.terminal;
                let look = preview::Look {
                    font: terminal.font(),
                    size: terminal.size,
                    line_ratio: terminal.line_height() / terminal.size,
                    cell_ratio: self.cell_width / terminal.size,
                    theme: theme.clone(),
                };
                pane.relative()
                    .bg(rgb(theme.background))
                    .child(preview::paint_screen(
                        surface,
                        rect,
                        palette.preview_painter.clone(),
                        look,
                    ))
                    .when_some(seen_at, |pane, at| {
                        let age = crate::ago::text(super::frecency::now().saturating_sub(at));
                        pane.child(
                            div()
                                .debug_selector(|| "palette-preview-age".into())
                                .absolute()
                                .top(px(8.))
                                .inset_e(px(8.))
                                .px(px(8.))
                                .py(px(2.))
                                .rounded(px(4.))
                                .bg(rgb(theme.surface))
                                .border_1()
                                .border_color(rgb(theme.active))
                                .text_color(rgb(theme.muted))
                                .child(fl!("palette-go-to-last-seen", age = age)),
                        )
                    })
            }
            Some(preview::Preview::Facts { title, facts }) => pane
                .p(px(16.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .children(facts.into_iter().map(|(name, value)| {
                    div()
                        .row()
                        .gap(px(12.))
                        .child(
                            div()
                                .flex_none()
                                .w(px(self.config.ui.size * 6.))
                                .text_color(rgb(theme.muted))
                                .child(name),
                        )
                        .child(div().flex_1().min_w_0().truncate().child(value))
                }))
                .child(
                    div()
                        .pt(px(10.))
                        .text_color(rgb(theme.muted))
                        .child(fl!("palette-not-on-screen")),
                ),
            None => pane,
        }
    }

    pub(crate) fn render_palette(&self, viewport: Size<Pixels>, cx: &mut Context<Self>) -> Div {
        let Some(palette) = &self.menu.palette else {
            return div();
        };
        let theme = &self.theme;
        let previews = self.palette_previews(viewport);
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(self.palette_header(palette, cx))
            .when(palette.loading_projects, |panel| {
                panel.child(
                    div()
                        .flex_none()
                        .px(px(16.))
                        .py(px(8.))
                        .text_color(rgb(theme.muted))
                        .child(fl!("palette-loading-projects")),
                )
            })
            .when(!palette.projects.errors.is_empty(), |panel| {
                panel.child(
                    div()
                        .id("palette-project-errors")
                        .max_h(px(72.))
                        .overflow_y_scroll()
                        .flex_none()
                        .px(px(16.))
                        .py(px(8.))
                        .text_color(rgb(theme.muted))
                        .child(
                            palette
                                .projects
                                .errors
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join("\n"),
                        ),
                )
            })
            .when_some(palette.error.clone(), |panel, error| {
                panel.child(
                    div()
                        .id("palette-error")
                        .max_h(px(90.))
                        .overflow_y_scroll()
                        .flex_none()
                        .p(px(12.))
                        .text_color(rgb(theme.foreground))
                        .bg(rgb(theme.active))
                        .child(error),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .row()
                    .when(palette.filtered.is_empty(), |body| {
                        body.child(
                            div()
                                .flex_1()
                                .p(px(16.))
                                .text_color(rgb(theme.muted))
                                .child(fl!("palette-no-results")),
                        )
                    })
                    .when(!palette.filtered.is_empty(), |body| {
                        body.child(
                            uniform_list(
                                "palette-results",
                                palette.filtered.len(),
                                cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                                    let Some(palette) = &this.menu.palette else {
                                        return Vec::new();
                                    };
                                    range
                                        .map(|index| this.palette_row(palette, index, cx))
                                        .collect()
                                }),
                            )
                            .track_scroll(&palette.scroll)
                            .flex_1()
                            .min_w_0(),
                        )
                    })
                    .when(previews, |body| {
                        body.child(self.render_palette_preview(palette))
                    }),
            )
            .child(
                div()
                    .debug_selector(|| "palette-status".into())
                    .flex_none()
                    .px(px(16.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_color(rgb(theme.muted))
                    .child(if palette.busy() {
                        fl!("palette-opening-project")
                    } else if palette.filter == Filter::Navigation {
                        fl!("palette-go-to-hints")
                    } else {
                        fl!("palette-hints")
                    }),
            )
    }
}
