//! The device picker's activity card: every device's agents over the last
//! two hours and how many work and wait now, above the device list. The
//! whole card opens the Devices overview, where the same history is drawn
//! in full.

use crate::{
    HerdrWindow,
    devices_overview::{Tally, sparkline},
};
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;

const SPARK_HEIGHT: f32 = 22.;
/// About what the card takes above the list at the default font size,
/// margins included: the list gives up this much in a window too short for
/// both. Only a budget; the card sizes itself.
pub(super) const ROOM: f32 = 100.;

impl HerdrWindow {
    pub(super) fn render_activity_card(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let look = self.overview_look(cx);
        let theme = look.theme;
        let small = px(self.config.ui.size * 0.85);
        let tally = Tally::sum(&self.overview_devices());
        let count = |status: AgentStatus, text: String| {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .child(look.dot(status))
                .child(text)
        };
        div()
            .id("device-activity")
            .debug_selector(|| "device-activity".into())
            .flex_none()
            .m(px(4.))
            .p(px(10.))
            .rounded(px(crate::config::corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.background))
            .cursor_pointer()
            .hover(|style| style.border_color(rgb(theme.muted)))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .text_size(small)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(theme.muted))
                            .child("AGENT ACTIVITY · 2 H"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(3.))
                            .text_color(rgb(theme.subtext()))
                            .child("Overview")
                            .child(
                                svg()
                                    .size(px(11.))
                                    .path("icons/arrow-right.svg")
                                    .text_color(rgb(theme.subtext())),
                            ),
                    ),
            )
            .child(sparkline(
                &self.devices_overview.history,
                &look,
                SPARK_HEIGHT,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(px(10.))
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(count(
                        AgentStatus::Working,
                        format!("{} working", tally.working),
                    ))
                    .child(count(
                        AgentStatus::Blocked,
                        format!("{} blocked", tally.blocked),
                    )),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_devices_overview(window, cx);
            }))
    }
}
