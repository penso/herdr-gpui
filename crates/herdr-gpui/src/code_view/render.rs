//! A code tab paints the read file and its outline; reading lives elsewhere.

use super::{CodeView, State, Text};
use crate::fonts::StyledFont;
use crate::{
    HerdrWindow,
    browser::{Slot, Tab, TabId},
};
use gpui::{prelude::*, *};
use std::{cell::Cell, rc::Rc};

/// The outline's share of the tab, so a narrow group keeps room for code.
const OUTLINE_WIDTH: f32 = 240.;
/// Below this width the outline stays hidden whatever its toggle says.
const OUTLINE_MIN_TAB_WIDTH: f32 = 640.;

/// Records a tab's width as it lays out, and draws again when that moves it
/// across the outline's threshold, so the outline follows on the next frame.
fn measure(width: Rc<Cell<f32>>) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            let now = f32::from(bounds.size.width);
            let before = width.replace(now);
            if (before >= OUTLINE_MIN_TAB_WIDTH) != (now >= OUTLINE_MIN_TAB_WIDTH) {
                window.refresh();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

impl HerdrWindow {
    fn code_line_height(&self) -> f32 {
        self.config.terminal.line_height().max(14.)
    }

    fn code_row(
        &self,
        id: TabId,
        view: &CodeView,
        text: &Text,
        row: usize,
        gutter: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let line = u32::try_from(row + 1).unwrap_or(u32::MAX);
        let spans = view.spans.get(row).map_or(&[][..], Vec::as_slice);
        div()
            .id(row)
            .debug_selector(move || format!("code-line-{row}"))
            .h(px(self.code_line_height()))
            .flex()
            .items_center()
            .whitespace_nowrap()
            .when(view.marked == Some(line), |row| row.bg(rgb(theme.active)))
            .child(
                div()
                    .flex_none()
                    .w(px(gutter))
                    .pr(px(12.))
                    .flex()
                    .justify_end()
                    .text_color(rgb(theme.muted))
                    .child(line.to_string()),
            )
            .child(crate::review::styled_code(
                theme,
                text.line(row),
                spans,
                &[],
                None,
                None,
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(view) = this.code_views.get_mut(&id) {
                    view.marked = Some(line);
                    cx.notify();
                }
            }))
    }

    fn code_outline(&self, id: TabId, text: &Text, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        div()
            .flex_none()
            .w(px(OUTLINE_WIDTH))
            .border_l_1()
            .border_color(rgb(theme.active))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .px(px(10.))
                    .py(px(6.))
                    .text_color(rgb(theme.muted))
                    .child(format!("Outline \u{00b7} {}", text.outline.len())),
            )
            .child(
                uniform_list(
                    "code-outline",
                    text.outline.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        let Some(text) = this.code_views.get(&id).and_then(CodeView::text) else {
                            return Vec::new();
                        };
                        let text = text.clone();
                        range
                            .filter_map(|index| {
                                let entry = text.outline.get(index)?;
                                let line = entry.line;
                                Some(
                                    div()
                                        .id(index)
                                        .debug_selector(move || format!("code-outline-{index}"))
                                        .px(px(10.))
                                        .py(px(2.))
                                        .flex()
                                        .gap(px(8.))
                                        .cursor_pointer()
                                        .hover(|row| row.bg(rgb(this.theme.active)))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .child(entry.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_color(rgb(this.theme.muted))
                                                .child(entry.kind.label()),
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(view) = this.code_views.get_mut(&id) {
                                                view.go_to(line);
                                                cx.notify();
                                            }
                                        })),
                                )
                            })
                            .collect()
                    }),
                )
                .flex_1()
                .min_h_0(),
            )
    }

    fn code_header(&self, id: TabId, tab: &Tab, view: &CodeView, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let path = tab
            .location
            .as_ref()
            .map(crate::browser::Location::display)
            .unwrap_or_default();
        let button = |label: &'static str, selector: &'static str| {
            div()
                .id(selector)
                .debug_selector(move || selector.into())
                .flex_none()
                .px_2()
                .py_1()
                .rounded(px(crate::config::corners::CONTROL))
                .cursor_pointer()
                .hover(|button| button.bg(rgb(theme.active)))
                .child(label)
        };
        div()
            .flex_none()
            .px(px(10.))
            .py(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(theme.muted))
                    .child(path),
            )
            .child(
                button(
                    if view.outline {
                        "Hide Outline"
                    } else {
                        "Outline"
                    },
                    "code-outline-toggle",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(view) = this.code_views.get_mut(&id) {
                        view.outline = !view.outline;
                        cx.notify();
                    }
                })),
            )
            .child(
                button("Open in Editor", "code-open-editor").on_click(
                    cx.listener(move |this, _, _, cx| this.open_code_view_in_editor(id, cx)),
                ),
            )
    }

    fn render_code(&self, id: TabId, tab: &Tab, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let Some(view) = self.code_views.get(&id) else {
            return div().into_any_element();
        };
        let body = match &view.state {
            State::Loading => div()
                .p_3()
                .text_color(rgb(theme.muted))
                .child("Reading file\u{2026}")
                .into_any_element(),
            State::Failed(error) => div()
                .debug_selector(|| "code-error".into())
                .p_3()
                .text_color(crate::menu::danger(theme))
                .child(error.clone())
                .into_any_element(),
            State::Loaded(text) => {
                let digits = text.lines.len().max(1).ilog10() + 1;
                let gutter = (digits as f32 + 2.) * self.config.terminal.size * 0.62;
                let outline = view.outline
                    && !text.outline.is_empty()
                    && view.width.get() >= OUTLINE_MIN_TAB_WIDTH;
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .text_font(&self.config.terminal)
                    .text_size(px(self.config.terminal.size))
                    .child(
                        uniform_list(
                            "code-lines",
                            text.lines.len(),
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                let Some(view) = this.code_views.get(&id) else {
                                    return Vec::new();
                                };
                                let Some(text) = view.text().cloned() else {
                                    return Vec::new();
                                };
                                range
                                    .map(|row| this.code_row(id, view, &text, row, gutter, cx))
                                    .collect()
                            }),
                        )
                        .with_width_from_item(Some(text.widest))
                        .with_horizontal_sizing_behavior(
                            ListHorizontalSizingBehavior::Unconstrained,
                        )
                        .track_scroll(&view.scroll)
                        .flex_1()
                        .min_w_0(),
                    )
                    .when(outline, |row| row.child(self.code_outline(id, text, cx)))
                    .into_any_element()
            }
        };
        div()
            .debug_selector(|| "code-view".into())
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(measure(view.width.clone()))
            .child(self.code_header(id, tab, view, cx))
            .child(body)
            .into_any_element()
    }

    /// A code tab, drawn in `slot` where a page would be.
    pub(crate) fn render_code_tab(
        &self,
        slot: Slot,
        tab: &Tab,
        gap: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id;
        let Some(view) = self.code_views.get(&id) else {
            return div()
                .id(SharedString::from(slot.selector("code-loading")))
                .size_full()
                .pl(px(gap))
                .into_any_element();
        };
        let focus = view.focus.clone();
        div()
            .id(SharedString::from(slot.selector("code-tab")))
            .size_full()
            .min_w_0()
            .pl(px(gap))
            .bg(rgb(self.theme.background))
            .track_focus(&focus)
            // The tab holds the keyboard while used, so typing never
            // reaches a terminal in another group.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if let Some(view) = this.code_views.get(&id) {
                        window.focus(&view.focus, cx);
                    }
                }),
            )
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                let focused = this
                    .code_views
                    .get(&id)
                    .is_some_and(|view| view.focus.is_focused(window));
                if focused && this.code_view_key(id, event, cx) {
                    cx.stop_propagation();
                }
            }))
            .child(self.render_code(id, tab, cx))
            .into_any_element()
    }
}
