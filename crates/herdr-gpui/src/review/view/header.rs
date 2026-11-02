//! The review's header: the panels' toggles, which changes show and against
//! what, and how they are drawn: unified or side by side, with or without
//! changes in whitespace alone, with a field to find text in them.
use super::{Agent, Layout, Review, panels};
use crate::i18n::fl;
use crate::{HerdrWindow, browser::TabId, review::diff::Scope};
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(super) fn render_review_header(
        &self,
        id: TabId,
        review: &Review,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let destination = match &review.agent {
            Some(Agent {
                label: Some(label), ..
            }) => fl!("review-notes-go-to", agent = label.as_str()),
            Some(Agent { label: None, .. }) => fl!("review-notes-go-to-agent"),
            None => fl!("review-no-agent"),
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(self.render_review_panel_toggle(id, review, panels::Panel::Files, cx))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fl!("review-title")),
            )
            .child(self.render_review_scope(id, review.scope, cx))
            .child(
                div()
                    .debug_selector(|| "review-against".into())
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_color(rgb(theme.muted))
                    .child(div().min_w_0().truncate().child(match review.scope {
                        Scope::Branch => {
                            fl!("review-against", branch = review.checkout.branch.as_str())
                        }
                        Scope::Uncommitted => review.checkout.branch.clone(),
                    }))
                    .when(review.scope == Scope::Branch, |against| {
                        against.child(self.render_review_base(id, review, cx))
                    }),
            )
            .when(review.batch_out, |header| {
                header.child(
                    div()
                        .debug_selector(|| "review-reading".into())
                        .flex_none()
                        .text_color(rgb(theme.muted))
                        .child(fl!("review-reading")),
                )
            })
            .child(
                self.review_icon_button(
                    "review-search-open",
                    "icons/search.svg",
                    fl!("review-find"),
                    review.search.open,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_review_search(id, window, cx);
                })),
            )
            .child(
                self.review_icon_button(
                    "review-whitespace",
                    "icons/whitespace.svg",
                    fl!("review-hide-whitespace"),
                    review.ignore_whitespace,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.toggle_review_whitespace(id, cx);
                })),
            )
            .child(self.render_review_layout(id, review.layout, cx))
            .child(self.render_review_panel_toggle(id, review, panels::Panel::Notes, cx))
            .child(
                div()
                    .debug_selector(|| "review-destination".into())
                    .flex_none()
                    .text_color(rgb(theme.muted))
                    .child(destination),
            )
    }

    /// A square icon button of the header, lit when `on`.
    fn review_icon_button(
        &self,
        name: &'static str,
        icon: &'static str,
        hint: String,
        on: bool,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let (foreground, surface) = (theme.foreground, theme.surface);
        div()
            .id(name)
            .debug_selector(move || name.into())
            .flex_none()
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(crate::config::corners::CONTROL))
            .cursor_pointer()
            .when(on, |button| button.bg(rgb(theme.active)))
            .hover(|button| button.bg(rgb(theme.active)))
            .child(svg().path(icon).size(px(14.)).text_color(rgb(if on {
                theme.foreground
            } else {
                theme.muted
            })))
            .tooltip(move |_, cx| {
                cx.new(|_| crate::usage::Hint {
                    text: hint.clone().into(),
                    foreground,
                    surface,
                })
                .into()
            })
    }

    /// Shows the diff unified or side by side; notes and the place stay.
    pub(crate) fn set_review_layout(&mut self, id: TabId, layout: Layout, cx: &mut Context<Self>) {
        if let Some(review) = self.reviews.get_mut(&id) {
            review.set_layout(layout);
        }
        cx.notify();
    }

    /// The two layout icons in the header.
    fn render_review_layout(&self, id: TabId, current: Layout, cx: &mut Context<Self>) -> Div {
        let button = |name: &'static str, icon: &'static str, hint: String, layout: Layout| {
            self.review_icon_button(name, icon, hint, layout == current)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.set_review_layout(id, layout, cx);
                }))
        };
        div()
            .flex()
            .flex_none()
            .gap_1()
            .child(button(
                "review-layout-unified",
                "icons/diff-unified.svg",
                fl!("review-layout-unified"),
                Layout::Unified,
            ))
            .child(button(
                "review-layout-split",
                "icons/diff-split.svg",
                fl!("review-layout-split"),
                Layout::Split,
            ))
    }

    fn render_review_scope(&self, id: TabId, current: Scope, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let segment = |name: &'static str, label: String, scope: Scope| {
            let chosen = scope == current;
            div()
                .id(name)
                .debug_selector(move || name.into())
                .px_2()
                .rounded(px(crate::config::corners::CONTROL))
                .cursor_pointer()
                .when(chosen, |segment| {
                    segment
                        .bg(rgb(theme.active))
                        .text_color(rgb(theme.foreground))
                })
                .when(!chosen, |segment| {
                    segment
                        .text_color(rgb(theme.muted))
                        .hover(|segment| segment.text_color(rgb(theme.foreground)))
                })
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.set_review_scope(id, scope, cx);
                }))
        };
        div()
            .flex()
            .flex_none()
            .gap_1()
            .child(segment(
                "review-scope-uncommitted",
                fl!("review-scope-uncommitted"),
                Scope::Uncommitted,
            ))
            .child(segment(
                "review-scope-branch",
                fl!("review-scope-branch"),
                Scope::Branch,
            ))
    }
}
