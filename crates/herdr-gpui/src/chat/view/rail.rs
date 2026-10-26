//! The rail beside the entries: the agent's plan and the files its tool
//! calls changed. It hides on request and in panes too narrow for it, and
//! the header then keeps the plan's progress at hand.

use super::{super::chats::ChatPane, Hues};
use crate::{HerdrWindow, config::corners, notifications::safe_text};
use gpui::{prelude::*, *};
use herdr_chat_protocol::PlanStatus;

/// Narrowest pane that shows the rail beside the entries.
const MIN_PANE: f32 = 640.;
const WIDTH: f32 = 240.;
/// Plan steps and changed files listed; the rest are counted.
const ROWS: usize = 24;

/// Whether a pane of `bounds` is wide enough for the rail.
pub(super) fn fits(bounds: Bounds<Pixels>) -> bool {
    bounds.size.width >= px(MIN_PANE)
}

/// The last two parts of a path, which name a file well enough in the rail.
fn short_path(path: &str) -> &str {
    let mut cuts = path.rmatch_indices(['/', '\\']).map(|(index, _)| index);
    cuts.nth(1).map_or(path, |cut| &path[cut + 1..])
}

impl HerdrWindow {
    fn chat_rail_has_content(chat: &ChatPane) -> bool {
        !chat.session.transcript().plan.is_empty() || !chat.prepared.changed().is_empty()
    }

    /// The header's rail button, and the plan's progress while the rail is
    /// not shown.
    pub(super) fn render_chat_rail_controls(
        &self,
        chat: &ChatPane,
        fits: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !Self::chat_rail_has_content(chat) {
            return Vec::new();
        }
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let plan = &chat.session.transcript().plan;
        let shown = fits && chat.rail;
        let mut controls = Vec::new();
        if !shown && !plan.is_empty() {
            let done = plan
                .iter()
                .filter(|step| step.status == PlanStatus::Completed)
                .count();
            let working = plan
                .iter()
                .any(|step| step.status == PlanStatus::InProgress);
            controls.push(
                div()
                    .debug_selector(|| "chat-plan-pill".into())
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(7.))
                    .h(px(20.))
                    .rounded_full()
                    .bg(rgb(theme.surface))
                    .text_size(px((self.chat_text_size() - 2.).max(10.)))
                    .text_color(rgb(theme.subtext()))
                    .child(
                        svg()
                            .path(if working {
                                "icons/status-solid-working.svg"
                            } else {
                                "icons/status-solid-done.svg"
                            })
                            .size(px(9.))
                            .text_color(rgb(if working { hues.yellow } else { hues.green })),
                    )
                    .child(format!("Plan {done}/{}", plan.len()))
                    .into_any_element(),
            );
        }
        if fits {
            let pane_id = chat.target.pane_id.clone();
            controls.push(
                div()
                    .id("chat-rail-toggle")
                    .debug_selector(|| "chat-rail-toggle".into())
                    .flex_none()
                    .flex()
                    .items_center()
                    .px(px(6.))
                    .h(px(22.))
                    .rounded(px(corners::CONTROL))
                    .cursor_pointer()
                    .when(shown, |button| button.bg(rgb(theme.active)))
                    .hover(|button| button.bg(rgb(theme.active)))
                    .child(
                        svg()
                            .path("icons/panel-right.svg")
                            .size(px(13.))
                            .text_color(rgb(theme.foreground)),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        if let Some(pane) = this.chats.pane_mut(&pane_id) {
                            pane.rail = !pane.rail;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        controls
    }

    pub(super) fn render_chat_rail(
        &self,
        chat: &ChatPane,
        bounds: Bounds<Pixels>,
    ) -> Option<Stateful<Div>> {
        if !fits(bounds) || !chat.rail || !Self::chat_rail_has_content(chat) {
            return None;
        }
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let small = self.chat_small_text();
        let section = |title: &'static str, count: String| {
            div()
                .flex()
                .text_size(px((self.chat_text_size() - 2.).max(10.)))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(theme.muted))
                .child(title)
                .child(div().flex_1())
                .child(count)
        };
        let more = |count: usize| {
            (count > ROWS).then(|| {
                div()
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(format!("and {} more", count - ROWS))
            })
        };
        let plan = &chat.session.transcript().plan;
        let plan_section = (!plan.is_empty()).then(|| {
            let done = plan
                .iter()
                .filter(|step| step.status == PlanStatus::Completed)
                .count();
            div()
                .debug_selector(|| "chat-plan".into())
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(section("PLAN", format!("{done}/{}", plan.len())))
                .children(plan.iter().take(ROWS).map(|step| {
                    let (icon, color) = match step.status {
                        PlanStatus::Completed => ("icons/status-solid-done.svg", hues.green),
                        PlanStatus::InProgress => ("icons/status-solid-working.svg", hues.yellow),
                        PlanStatus::Pending => ("icons/status-outline-idle.svg", theme.muted),
                    };
                    div()
                        .flex()
                        .gap(px(6.))
                        .text_size(small)
                        .child(
                            svg()
                                .flex_none()
                                .mt(px(2.))
                                .path(icon)
                                .size(px(11.))
                                .text_color(rgb(color)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .when(step.status == PlanStatus::Completed, |text| {
                                    text.text_color(rgb(theme.subtext())).line_through()
                                })
                                .when(step.status == PlanStatus::InProgress, |text| {
                                    text.font_weight(FontWeight::SEMIBOLD)
                                })
                                .child(safe_text(&step.content, 300)),
                        )
                }))
                .children(more(plan.len()))
        });
        let changed = chat.prepared.changed();
        let files = (!changed.is_empty()).then(|| {
            let count = match changed.len() {
                1 => "1 file".to_owned(),
                count => format!("{count} files"),
            };
            div()
                .debug_selector(|| "chat-changed".into())
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(section("CHANGED", count))
                .children(changed.iter().take(ROWS).map(|diff| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(small)
                        .child(
                            svg()
                                .flex_none()
                                .path("icons/file.svg")
                                .size(px(11.))
                                .text_color(rgb(theme.subtext())),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(safe_text(short_path(&diff.path), 200)),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(hues.green))
                                .child(format!("+{}", diff.added)),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(hues.red))
                                .child(format!("\u{2212}{}", diff.removed)),
                        )
                }))
                .children(more(changed.len()))
        });
        Some(
            div()
                .id("chat-rail")
                .debug_selector(|| "chat-rail".into())
                .flex_none()
                .w(px(WIDTH))
                .h_full()
                .flex()
                .flex_col()
                .gap(px(16.))
                .p(px(12.))
                .overflow_y_scroll()
                .border_l_1()
                .border_color(rgb(theme.active))
                .bg(rgb(theme.surface))
                .children(plan_section)
                .children(files),
        )
    }
}

#[cfg(test)]
mod tests;
