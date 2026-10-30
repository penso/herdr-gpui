//! The sign that the agent is still on a turn, under the conversation in
//! every layout: a pulsing dot, what the agent is doing now, and how long
//! the turn has run. It sits outside the list, so it shows however far the
//! conversation is scrolled back.

use super::layout::Look;
use crate::motion;
use gpui::{prelude::*, *};
use herdr_chat_protocol::{EntryKind, Status, ToolStatus, Transcript};
use std::time::Duration;

/// One beat of the dot's pulse.
const PULSE: Duration = Duration::from_millis(1200);

/// What the agent is doing while a turn runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Working {
    /// Its activity, such as "Thinking" or the command a step runs.
    pub(super) doing: String,
    /// How long the turn has run, once it is a second or more.
    pub(super) elapsed: Option<String>,
}

impl Working {
    /// What `transcript` shows the agent doing, or `None` while it is not
    /// on a turn.
    pub(super) fn of(transcript: &Transcript, now_ms: u64) -> Option<Self> {
        if transcript.status != Status::Working {
            return None;
        }
        let doing = match transcript.entries.last().map(|entry| &entry.kind) {
            Some(EntryKind::Tool(tool))
                if matches!(tool.status, ToolStatus::Pending | ToolStatus::InProgress) =>
            {
                step_title(&tool.title)
            }
            Some(EntryKind::Agent { .. }) => "Writing".to_owned(),
            _ => "Thinking".to_owned(),
        };
        let elapsed = transcript
            .turn
            .map(|turn| now_ms.saturating_sub(turn.started_ms))
            .filter(|elapsed| *elapsed >= 1000)
            .map(herdr_chat_protocol::display::duration);
        Some(Self { doing, elapsed })
    }
}

/// A step's title as one plain line: an agent often wraps a command in
/// code ticks.
fn step_title(title: &str) -> String {
    let line = title.lines().next().unwrap_or_default().trim();
    let line = line
        .strip_prefix('`')
        .and_then(|inner| inner.strip_suffix('`'))
        .filter(|inner| !inner.contains('`'))
        .unwrap_or(line);
    if line.is_empty() {
        "Working".to_owned()
    } else {
        line.to_owned()
    }
}

/// A dot in the theme's accent that pulses while the agent works.
pub(super) fn pulse(look: &Look<'_>) -> impl IntoElement {
    div()
        .flex_none()
        .size(px((look.text * 0.42).max(6.)))
        .rounded_full()
        .bg(rgb(look.theme.primary()))
        .with_animation(
            "chat-working-pulse",
            Animation::new(motion::duration(PULSE)).repeat(),
            |dot, k| {
                // Brightest mid-beat, never quite gone.
                let k = if cfg!(test) { 0.5 } else { k };
                let wave = 1. - (2. * k - 1.).powi(2);
                dot.opacity(0.3 + 0.7 * wave)
            },
        )
}

/// The working row every layout shows unless it draws its own: blank but
/// the same height between turns, so the list above never changes size as
/// turns start and end.
pub(super) fn row(working: Option<&Working>, look: &Look<'_>) -> Div {
    let theme = look.theme;
    let small = (look.text - 1.).max(10.);
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(16.))
        .py(px(4.))
        .h(px(small * 1.6 + 8.))
        .text_size(px(small))
        .map(|row| match working {
            None => row.invisible(),
            Some(working) => row
                .debug_selector(|| "chat-working".into())
                .child(pulse(look))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.subtext()))
                        .child(format!("{}\u{2026}", working.doing)),
                )
                .children(working.elapsed.clone().map(|elapsed| {
                    div()
                        .flex_none()
                        .text_color(rgb(theme.muted))
                        .child(elapsed)
                })),
        })
}

#[cfg(test)]
mod tests;
