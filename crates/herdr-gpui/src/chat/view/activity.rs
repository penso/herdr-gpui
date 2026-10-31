//! A shared activity strip for every chat layout. Execution state comes from
//! the live connection/daemon; transcript entries only describe reported work.

use super::{ChatInput, ChatPane, Hues, text_ink};
use crate::{HerdrWindow, config::Theme};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{
    EntryKind, Status, ToolKind, ToolStatus, Transcript, TurnEnd, TurnSummary, display,
};
use std::time::Duration;

const RECENT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Starting,
    Idle,
    Working,
    Thinking,
    Running,
    Waiting,
    Completed,
    Stopped,
    Failed,
    Reconnecting,
}

impl Phase {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Starting => "Starting",
            Self::Idle => "Ready",
            Self::Working => "Working",
            Self::Thinking => "Thinking",
            Self::Running => "Running",
            Self::Waiting => "Needs input",
            Self::Completed => "Completed",
            Self::Stopped => "Stopped",
            Self::Failed => "Failed",
            Self::Reconnecting => "Reconnecting",
        }
    }

    pub(super) fn moving(self) -> bool {
        matches!(self, Self::Working | Self::Thinking | Self::Running)
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Working => "icons/settings.svg",
            Self::Thinking => "icons/pulse.svg",
            Self::Running => "icons/terminal.svg",
            Self::Waiting => "icons/status-orca-blocked.svg",
            Self::Completed => "icons/check.svg",
            Self::Failed => "icons/error.svg",
            Self::Stopped => "icons/x.svg",
            Self::Reconnecting => "icons/refresh.svg",
            Self::Starting => "icons/play.svg",
            Self::Idle => "icons/status-outline-idle.svg",
        }
    }

    pub(super) fn color(self, theme: &Theme) -> u32 {
        let hues = Hues::of(theme);
        let color = match self {
            Self::Working | Self::Running => hues.blue,
            Self::Thinking => hues.purple,
            Self::Waiting | Self::Stopped | Self::Reconnecting => hues.yellow,
            Self::Completed => hues.green,
            Self::Failed => hues.red,
            Self::Starting | Self::Idle => theme.subtext(),
        };
        text_ink(color, theme.background)
    }
}

pub(super) struct Activity {
    pub(super) phase: Phase,
    pub(super) detail: Option<String>,
    pub(super) metrics: Vec<String>,
}

/// Bounded presentation of the latest reported activity. A historical thought
/// or unfinished tool is never enough to turn an idle/disconnected agent busy.
pub(super) fn project(
    transcript: &Transcript,
    attached: bool,
    needs_input: bool,
    now_ms: u64,
) -> Activity {
    let recent = || transcript.entries.iter().rev().take(RECENT);
    let last_turn = recent().find_map(|entry| match &entry.kind {
        EntryKind::Turn(summary) => Some(summary),
        _ => None,
    });
    let latest = recent().find(|entry| !matches!(entry.kind, EntryKind::Notice { .. }));
    let phase = if !attached {
        Phase::Reconnecting
    } else {
        match &transcript.status {
            Status::Failed { .. } => Phase::Failed,
            Status::Exited { .. } => Phase::Stopped,
            _ if needs_input || transcript.is_blocked() => Phase::Waiting,
            Status::Starting => Phase::Starting,
            Status::Idle => match latest.map(|entry| &entry.kind) {
                Some(EntryKind::Turn(summary)) => match summary.end {
                    TurnEnd::Done => Phase::Completed,
                    TurnEnd::Failed => Phase::Failed,
                    TurnEnd::Other => Phase::Idle,
                    _ => Phase::Stopped,
                },
                _ => Phase::Idle,
            },
            // Native prompts append a user entry before starting their turn.
            // Observed logs expose a clock but no corresponding entry boundary.
            Status::Working if !transcript.read_only && transcript.turn.is_some() => {
                match latest.map(|entry| &entry.kind) {
                    Some(EntryKind::Thought { .. }) => Phase::Thinking,
                    Some(EntryKind::Tool(tool)) if tool.status == ToolStatus::InProgress => {
                        if tool.kind == ToolKind::Think {
                            Phase::Thinking
                        } else {
                            Phase::Running
                        }
                    }
                    _ => Phase::Working,
                }
            }
            Status::Working => Phase::Working,
        }
    };
    let detail = match phase {
        Phase::Waiting => Some(if transcript.read_only {
            "Answer in the terminal below".into()
        } else {
            "Review the request below".into()
        }),
        Phase::Failed | Phase::Stopped => match &transcript.status {
            Status::Failed { message } | Status::Exited { message } if !message.is_empty() => {
                Some(crate::notifications::safe_text(message, 160))
            }
            _ => None,
        },
        Phase::Reconnecting => Some("Waiting for live updates".into()),
        _ if phase.moving() => recent()
            .take_while(|entry| !matches!(entry.kind, EntryKind::User { .. } | EntryKind::Turn(_)))
            .find_map(|entry| match &entry.kind {
                EntryKind::Tool(tool) => Some(format!(
                    "{}: {}",
                    if transcript.read_only || transcript.turn.is_none() {
                        "Last reported tool"
                    } else if phase == Phase::Running {
                        "Current tool"
                    } else {
                        "Last tool"
                    },
                    crate::notifications::safe_text(&tool.title, 140)
                )),
                _ => None,
            }),
        _ => None,
    };
    let mut metrics = Vec::new();
    if phase.moving()
        && let Some(elapsed) = transcript
            .turn
            .and_then(|turn| now_ms.checked_sub(turn.started_ms))
        && elapsed >= 1000
    {
        metrics.push(format!("Elapsed {}", display::duration(elapsed)));
    }
    if let Some(usage) = &transcript.usage
        && usage.size > 0
    {
        metrics.push(format!(
            "Context {} / {}",
            display::tokens(usage.used),
            display::tokens(usage.size)
        ));
    }
    if let Some(summary) = last_turn {
        metrics.push(completed_metrics(summary));
    }
    Activity {
        phase,
        detail,
        metrics,
    }
}

fn completed_metrics(summary: &TurnSummary) -> String {
    let mut text = format!("Last turn: {}", display::duration(summary.duration_ms));
    if let Some(tokens) = &summary.tokens {
        text.push_str(&format!(
            " · {} output tokens",
            display::tokens(tokens.output)
        ));
        if tokens.thought > 0 {
            text.push_str(&format!(" · {} thinking", display::tokens(tokens.thought)));
        }
    }
    text
}

impl HerdrWindow {
    pub(in crate::chat) fn chat_needs_input(&self, chat: &ChatPane) -> bool {
        chat.session.transcript().is_blocked()
            || (chat.input() != ChatInput::Composer
                && self.live.agent_status(&chat.target.pane_id)
                    == Some(herdr_client::protocol::AgentStatus::Blocked))
    }

    pub(super) fn chat_activity(&self, chat: &ChatPane) -> Activity {
        project(
            chat.session.transcript(),
            chat.attached(),
            self.chat_needs_input(chat),
            super::super::now_ms(),
        )
    }

    pub(super) fn render_chat_activity(&self, chat: &ChatPane) -> Div {
        let Activity {
            phase,
            detail,
            metrics,
        } = self.chat_activity(chat);
        let theme = &self.theme;
        let color = phase.color(theme);
        let marker = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(3.))
            .w(px(25.))
            .when(phase.moving(), |marker| {
                marker.debug_selector(|| "chat-live-moving".into())
            })
            .children((0..3).map(|index| {
                let dot = div().size(px(5.)).rounded_full().bg(rgb(color));
                if phase.moving() {
                    dot.with_animation(
                        ElementId::Name(format!("chat-live-dot-{index}").into()),
                        Animation::new(crate::motion::duration(Duration::from_millis(1200)))
                            .repeat_synced()
                            .with_max_fps(12.),
                        move |dot, delta| dot.opacity(pulse(delta, index)),
                    )
                    .into_any_element()
                } else {
                    dot.opacity(if index == 0 { 1. } else { 0.25 })
                        .into_any_element()
                }
            }));
        div()
            .debug_selector(|| "chat-live-activity".into())
            .flex_none()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(4.))
            .px(px(12.))
            .py(px(7.))
            .min_h(px(36.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .text_size(px((self.chat_text_size() - 2.).max(10.)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(
                        svg()
                            .path(phase.icon())
                            .size(px(13.))
                            .flex_none()
                            .text_color(rgb(color)),
                    )
                    .child(
                        div()
                            .debug_selector(|| "chat-live-phase".into())
                            .flex_none()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(color))
                            .child(phase.label()),
                    )
                    .when(phase.moving(), |row| {
                        row.child(
                            div()
                                .debug_selector(|| "chat-live-indicator".into())
                                .child(marker),
                        )
                    })
                    .children(detail.map(|text| {
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(theme.subtext()))
                            .child(text)
                    }))
                    .map(|row| {
                        // Preserve the layout's native typing cue without a second
                        // working strip; shared usage metrics remain below it.
                        if phase.moving()
                            && self.config.chat.layout == crate::config::ChatLayoutMode::IMessage
                        {
                            let working = super::working::Working::of(
                                chat.session.transcript(),
                                super::super::now_ms(),
                            );
                            super::layout::layout_for(self.config.chat.layout)
                                .working(working.as_ref(), &self.chat_look())
                                .unwrap_or(row)
                        } else {
                            row
                        }
                    }),
            )
            .when(!metrics.is_empty(), |row| {
                row.child(
                    div()
                        .debug_selector(|| "chat-live-metrics".into())
                        .flex()
                        .flex_wrap()
                        .gap_x(px(12.))
                        .gap_y(px(3.))
                        .text_color(rgb(theme.subtext()))
                        .children(metrics.into_iter().map(|text| div().min_w_0().child(text))),
                )
            })
    }
}

fn pulse(delta: f32, index: u32) -> f32 {
    let phase = delta - index as f32 / 3.;
    0.58 + 0.42 * (phase * std::f32::consts::TAU).cos()
}

#[cfg(test)]
mod tests;
