//! What a row shows beneath itself: the last lines of a blocked agent's pane,
//! and the replies queued for the agent.

use super::{
    Row, State,
    replies::{Phase, Reply},
};
use crate::{SheprdWindow, devices_overview::Look, i18n::fl};
use gpui::{prelude::*, *};

/// Lines up with the row's title: its padding, dot and icon columns, and gaps.
const INSET: f32 = 59.;

impl SheprdWindow {
    /// What goes beneath `row`, when anything does. `active` is whether the
    /// row is in the live list rather than on a shelf.
    pub(super) fn overview_row_notes(
        &self,
        look: &Look,
        row: &Row,
        active: bool,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let (endpoint, pane) = row.pane()?;
        let excerpt = self
            .overview
            .excerpts
            .iter()
            .find(|excerpt| excerpt.endpoint == endpoint && excerpt.pane == pane)
            .filter(|_| active && row.state == State::Blocked);
        let replies: Vec<_> = self.overview.replies.of(endpoint, pane).cloned().collect();
        if excerpt.is_none() && replies.is_empty() {
            return None;
        }
        let theme = look.theme;
        let selector = format!("overview-notes-{}", row.key.selector());
        let mut notes = div()
            .debug_selector(move || selector.clone())
            .flex_none()
            .pl(px(INSET))
            .pr_2()
            .pb_2()
            .flex()
            .flex_col()
            .gap_1();
        if let Some(excerpt) = excerpt {
            let selector = format!("overview-excerpt-{}", row.key.selector());
            notes = notes.child(
                div()
                    .debug_selector(move || selector.clone())
                    .px_2()
                    .py_1()
                    .flex()
                    .flex_col()
                    .rounded(px(crate::config::corners::SMALL))
                    .bg(rgb(theme.active))
                    .children(excerpt.lines.iter().map(|line| {
                        look.mono(line.clone())
                            .text_color(rgb(theme.subtext()))
                            .truncate()
                    })),
            );
        }
        for reply in replies {
            notes = notes.child(self.overview_reply_line(look, reply, cx));
        }
        Some(notes)
    }

    fn overview_reply_line(&self, look: &Look, reply: Reply, cx: &mut Context<Self>) -> Div {
        let theme = look.theme;
        let id = reply.id;
        let (label, dismissable) = match &reply.phase {
            Phase::Waiting => (fl!("overview-reply-waiting"), true),
            Phase::Sending => (fl!("overview-reply-sending"), false),
            Phase::Failed(why) => (fl!("overview-reply-failed", error = why.as_str()), true),
        };
        let name = format!("overview-reply-cancel-{id}");
        let line = format!("overview-reply-{id}");
        div()
            .debug_selector(move || line.clone())
            .flex()
            .items_center()
            .gap_2()
            .text_size(look.small())
            .text_color(rgb(theme.muted))
            .child(
                svg()
                    .path("icons/arrow-right.svg")
                    .size(px(11.))
                    .flex_none()
                    .text_color(rgb(theme.muted)),
            )
            .child(div().flex_none().child(label))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .text_color(rgb(theme.subtext()))
                    .child(crate::notifications::safe_text(&reply.text, 200)),
            )
            .when(dismissable, |line| {
                line.child(
                    super::view::pill(name, "icons/x.svg", fl!("overview-reply-cancel"), look)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if this.overview.replies.cancel(id) {
                                cx.notify();
                            }
                        })),
                )
            })
    }
}
