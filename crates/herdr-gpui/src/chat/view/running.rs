//! Sub-agents and background shells the agent left running, said in words
//! on the working row ("2 sub-agents · 1 background shell"), and a table of
//! each one under it: what it is, how long it has run, and how it ended.
//! Shells outlive turns, so the row stays while anything still runs after
//! the agent goes idle, and clears once everything is done.

use super::layout::Look;
use crate::{HerdrWindow, chat::chats::ChatPane, motion};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{Activity, ActivityKind, ActivityState, Transcript, display};

/// Rows the table shows before it scrolls.
const ROWS: usize = 8;

/// What runs beside the conversation, while anything does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Running {
    /// What still runs, such as "2 sub-agents · 1 background shell".
    pub(crate) summary: String,
    /// Every activity the transcript keeps: what runs first, in the order it
    /// started, then what ended, most recent first.
    pub(crate) rows: Vec<RunningRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunningRow {
    pub(crate) detail: Option<String>,
    /// How long it has run, or ran.
    pub(crate) elapsed: String,
    pub(crate) kind: ActivityKind,
    pub(crate) label: String,
    pub(crate) state: ActivityState,
}

impl Running {
    /// What `transcript` has running at `now_ms`, or `None` once nothing
    /// does.
    pub(crate) fn of(transcript: &Transcript, now_ms: u64) -> Option<Self> {
        let running = transcript.running();
        if running.total() == 0 {
            return None;
        }
        let count = |count: usize, one: &str, many: &str| {
            (count > 0).then(|| format!("{count} {}", if count == 1 { one } else { many }))
        };
        let summary = [
            count(running.subagents, "sub-agent", "sub-agents"),
            count(running.shells, "background shell", "background shells"),
            count(running.other, "task", "tasks"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" \u{b7} ");
        let row = |activity: &Activity| RunningRow {
            detail: activity.detail.clone(),
            elapsed: display::duration(
                activity
                    .finished_ms
                    .unwrap_or(now_ms)
                    .saturating_sub(activity.started_ms),
            ),
            kind: activity.kind,
            label: activity.label.clone(),
            state: activity.state,
        };
        let mut ended: Vec<&Activity> = transcript
            .activities
            .iter()
            .filter(|activity| !activity.is_running())
            .collect();
        ended.sort_by_key(|activity| std::cmp::Reverse(activity.finished_ms));
        let rows = transcript
            .activities
            .iter()
            .filter(|activity| activity.is_running())
            .chain(ended)
            .map(row)
            .collect();
        Some(Self { summary, rows })
    }
}

/// The word for how an activity stands, and the color it is shown in.
fn outcome(state: ActivityState, look: &Look<'_>) -> (&'static str, u32) {
    let theme = look.theme;
    match state {
        // As a running call's card says it.
        ActivityState::Running => ("Running", theme.ink(theme.palette[3])),
        ActivityState::Finished => ("Done", theme.ink(theme.palette[2])),
        ActivityState::Failed => ("Failed", theme.ink(theme.palette[1])),
        ActivityState::Killed => ("Stopped", theme.muted),
    }
}

fn icon(kind: ActivityKind) -> &'static str {
    match kind {
        ActivityKind::Subagent => "icons/fan-out.svg",
        ActivityKind::Shell => "icons/terminal.svg",
        ActivityKind::Other => "icons/pulse.svg",
    }
}

/// The working row's height, which this row matches so nothing moves.
fn row_height(look: &Look<'_>) -> f32 {
    small(look) * 1.6 + 8.
}

fn small(look: &Look<'_>) -> f32 {
    (look.text - 1.).max(10.)
}

impl HerdrWindow {
    /// The working row with what still runs said after it, or alone, with
    /// its own pulse, once the agent is idle.
    pub(super) fn render_chat_running_row(
        &self,
        chat: &ChatPane,
        working: Option<Div>,
        idle: bool,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let look = self.chat_look();
        let Some(running) = Running::of(chat.session.transcript(), crate::chat::now_ms()) else {
            return working;
        };
        let theme = look.theme;
        let open = chat.running.is_open();
        let pane = chat.target.pane_id.clone();
        let text = if idle {
            format!("{} still running", running.summary)
        } else {
            running.summary
        };
        let words = div()
            .id("chat-running")
            .debug_selector(|| "chat-running".into())
            .flex()
            .items_center()
            .gap(px(6.))
            .min_w_0()
            .h(px(row_height(&look)))
            .text_size(px(small(&look)))
            .cursor_pointer()
            .text_color(rgb(theme.subtext()))
            .hover(|words| words.text_color(rgb(theme.foreground)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if let Some(chat) = this.chats.pane_mut(&pane) {
                    let open = !chat.running.is_open();
                    chat.running.set(open, motion::now(), false);
                }
                cx.notify();
            }))
            .when(idle, |words| words.child(super::working::pulse(&look)))
            .when(!idle, |words| {
                words.child(div().text_color(rgb(theme.muted)).child("\u{b7}"))
            })
            .child(div().min_w_0().truncate().child(text))
            .child(
                svg()
                    .flex_none()
                    .size(px(10.))
                    .path(if open {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    })
                    .text_color(rgb(theme.muted)),
            );
        Some(match working.filter(|_| !idle) {
            Some(working) => div()
                .flex()
                .items_center()
                .min_w_0()
                .pr(px(16.))
                .child(working.flex_none().max_w(relative(0.6)))
                .child(words),
            None => div().flex().items_center().px(px(16.)).child(words),
        })
    }

    /// The table of what runs and what ended, opened from the row; it grows
    /// in and shrinks out, from where it is when turned around.
    pub(super) fn render_chat_running_table(&self, chat: &ChatPane) -> Option<AnyElement> {
        let look = self.chat_look();
        let now = motion::now();
        let running = Running::of(chat.session.transcript(), crate::chat::now_ms());
        let shown = if running.is_some() {
            chat.running.shown(now)
        } else {
            0.
        };
        let running = running.filter(|_| shown > 0.)?;
        let theme = look.theme;
        let size = small(&look);
        let line = row_height(&look);
        let visible = running.rows.len().min(ROWS);
        let full = line * (visible as f32 + 1.) + 6.;
        let header = |label: &'static str| div().text_color(rgb(theme.muted)).child(label);
        let status = px(64.);
        let rows = running.rows.iter().enumerate().map(|(index, row)| {
            let (word, color) = outcome(row.state, &look);
            div()
                .debug_selector(move || format!("chat-running-row-{index}"))
                .flex()
                .items_center()
                .gap(px(10.))
                .h(px(line))
                .when(row.state != ActivityState::Running, |row| row.opacity(0.65))
                .child(
                    svg()
                        .flex_none()
                        .size(px(size))
                        .path(icon(row.kind))
                        .text_color(rgb(theme.subtext())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .gap(px(6.))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(theme.foreground))
                                .child(row.label.clone()),
                        )
                        .children(row.detail.clone().map(|detail| {
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(theme.muted))
                                .child(format!("\u{b7} {detail}"))
                        })),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.muted))
                        .child(row.elapsed.clone()),
                )
                .child(
                    div()
                        .flex_none()
                        .w(status)
                        .flex()
                        .justify_end()
                        .text_color(rgb(color))
                        .child(word),
                )
        });
        let table = div()
            .debug_selector(|| "chat-running-table".into())
            .flex()
            .flex_col()
            .mx(px(16.))
            .pt(px(4.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .text_size(px(size))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .h(px(line))
                    .text_size(px(size - 1.))
                    .child(div().flex_none().w(px(size)))
                    .child(div().flex_1().child(header("What")))
                    .child(header("Time"))
                    .child(div().w(status).flex().justify_end().child(header("Status"))),
            )
            .child(
                div()
                    .id("chat-running-rows")
                    .flex()
                    .flex_col()
                    .max_h(px(line * ROWS as f32))
                    .overflow_y_scroll()
                    .children(rows),
            );
        Some(
            div()
                .overflow_hidden()
                .max_h(px(full * shown))
                .opacity(shown)
                .child(table)
                .when(chat.running.moving(now), |frame| {
                    // Draws the next frame while it is still moving.
                    frame.child(
                        canvas(
                            |_, window, _| window.request_animation_frame(),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_0(),
                    )
                })
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests;
