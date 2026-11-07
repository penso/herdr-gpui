//! The sign that the agent is still on a turn, under the conversation in
//! every layout: a pulsing dot, what the agent is doing now, how long that
//! step and the turn have run, and on a click every step running, in full.
//! It sits outside the list, so it shows however far the conversation is
//! scrolled back.

use super::{Hues, layout::Look, rail::short_path};
use crate::{
    SheprdWindow,
    chat::chats::{ChatInput, ChatPane},
    i18n::fl,
    motion,
    notifications::safe_text,
};
use gpui::{prelude::*, *};
use sheprd_chat_protocol::{EntryKind, Status, Tool, ToolKind, ToolStatus, Transcript};
use std::time::Duration;

/// One beat of the dot's pulse.
const PULSE: Duration = Duration::from_millis(1200);

/// Steps the open list keeps; it says how many more run.
const MAX_STEPS: usize = 16;

/// Characters of one step's title the open list draws: a command can carry
/// a whole script, and this is drawn every frame.
const MAX_STEP_TEXT: usize = 2000;

/// Files of one step the open list names.
const MAX_STEP_LOCATIONS: usize = 8;

/// Characters of a file or of the row's one line, as a tool's card shows them.
const MAX_LINE: usize = 300;

/// Lines the open list shows before it scrolls.
const LINES: f32 = 6.;

/// The mark of an agent waiting on the user, which holds still: a pulse
/// would say it is busy.
pub(super) const WAITING_ICON: &str = "icons/status-orca-blocked.svg";

/// The kind of thing the agent is doing, which picks the row's mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    /// A step runs.
    Running,
    Thinking,
    /// It is stopped on the user's answer, though its turn still runs.
    Waiting,
    /// Busy, with nothing to say what at: all an observed agent's log tells
    /// between the records it saves.
    Working,
    Writing,
}

impl Phase {
    fn color(self, look: &Look<'_>) -> u32 {
        let hues = Hues::of(look.theme);
        match self {
            Self::Running => hues.blue,
            Self::Thinking => hues.purple,
            Self::Waiting => hues.yellow,
            Self::Working | Self::Writing => look.theme.primary(),
        }
    }
}

/// What the agent is doing while a turn runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Working {
    /// Its activity, such as "Thinking" or the command a step runs.
    pub(super) doing: String,
    /// How long the turn has run, once it is a second or more.
    pub(super) elapsed: Option<String>,
    /// How many other steps run beside the one `doing` names. A sub-agent
    /// is not one of them: the running row counts those.
    pub(super) more: usize,
    /// Whether the steps show in full under the row, which the pane knows.
    pub(super) open: bool,
    pub(super) phase: Phase,
    /// How long the step `doing` names has run, once it is a second or more.
    pub(super) step: Option<String>,
}

/// A running step, in full.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Step {
    /// How long it has run, once it is a second or more.
    pub(super) elapsed: Option<String>,
    pub(super) text: String,
}

/// What the open list shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Steps {
    /// How many more run than the list keeps, which it says rather than
    /// look complete.
    pub(super) hidden: usize,
    /// The steps running, the one [`Working`] names first.
    pub(super) list: Vec<Step>,
}

impl Steps {
    /// The steps `transcript` has running. Only the open list asks, as this
    /// is more than a closed row needs each frame.
    pub(super) fn of(transcript: &Transcript, now_ms: u64) -> Self {
        let running = running_steps(transcript);
        Self {
            hidden: running.len().saturating_sub(MAX_STEPS),
            list: running
                .into_iter()
                .take(MAX_STEPS)
                .map(|tool| Step {
                    elapsed: tool.started_ms.and_then(|started| since(started, now_ms)),
                    text: step_text(tool),
                })
                .collect(),
        }
    }
}

/// How long ago `started_ms` was, once it is a second or more.
fn since(started_ms: u64, now_ms: u64) -> Option<String> {
    Some(now_ms.saturating_sub(started_ms))
        .filter(|elapsed| *elapsed >= 1000)
        .map(crate::chat::text::duration)
}

impl Working {
    /// What `transcript` shows the agent doing, or `None` while it is not
    /// on a turn.
    pub(super) fn of(transcript: &Transcript, now_ms: u64) -> Option<Self> {
        if transcript.status != Status::Working {
            return None;
        }
        let running = running_steps(transcript);
        let (phase, doing) = match (running.first(), transcript.entries.last()) {
            (Some(tool), _) if tool.kind == ToolKind::Think => (Phase::Thinking, step_doing(tool)),
            (Some(tool), _) => (Phase::Running, step_doing(tool)),
            // A log saves whole records, so its last says nothing of now.
            _ if transcript.read_only => (Phase::Working, fl!("chat-working")),
            (None, Some(entry)) if matches!(entry.kind, EntryKind::Agent { .. }) => {
                (Phase::Writing, fl!("chat-working-writing"))
            }
            _ => (Phase::Thinking, fl!("chat-working-thinking")),
        };
        Some(Self {
            doing,
            elapsed: transcript
                .turn
                .and_then(|turn| since(turn.started_ms, now_ms)),
            more: running
                .iter()
                .filter(|tool| tool.kind != ToolKind::Subagent)
                .count()
                .saturating_sub(1),
            open: false,
            phase,
            step: running
                .first()
                .and_then(|tool| tool.started_ms)
                .and_then(|started| since(started, now_ms)),
        })
    }

    /// The same turn stopped on the user's answer, saying where `input` has
    /// them give it. The header says that it waits; this says what to do.
    pub(super) fn waiting(self, input: ChatInput) -> Self {
        let doing = match input {
            ChatInput::Composer => fl!("chat-activity-review"),
            ChatInput::ReadOnly => fl!("chat-activity-answer-terminal"),
            ChatInput::Terminal => fl!("chat-activity-answer-below"),
        };
        Self {
            doing,
            phase: Phase::Waiting,
            ..self
        }
    }
}

/// The steps of this turn still running, newest first, sub-agents last: one
/// waits on its own steps, which say more. An earlier turn's unfinished call
/// does not count, as the agent will not finish it.
fn running_steps(transcript: &Transcript) -> Vec<&Tool> {
    let (mut steps, subagents): (Vec<&Tool>, Vec<&Tool>) = transcript
        .entries
        .iter()
        .rev()
        .take_while(|entry| !matches!(entry.kind, EntryKind::User { .. } | EntryKind::Turn(_)))
        .filter_map(|entry| match &entry.kind {
            EntryKind::Tool(tool)
                if matches!(tool.status, ToolStatus::Pending | ToolStatus::InProgress) =>
            {
                Some(tool)
            }
            _ => None,
        })
        .partition(|tool| tool.kind != ToolKind::Subagent);
    steps.extend(subagents);
    steps
}

/// A step in a few words: what it does to the file it names, which a long
/// title would cut off, or else its title. A command's locations are where
/// it runs, not what it works on.
fn step_doing(tool: &Tool) -> String {
    let target = tool
        .locations
        .first()
        .map(|path| safe_text(short_path(path), MAX_LINE))
        .filter(|target| !target.is_empty());
    let Some(target) = target else {
        return step_title(&tool.title);
    };
    let target = target.as_str();
    match tool.kind {
        ToolKind::Delete => fl!("chat-working-deleting", target = target),
        ToolKind::Edit => fl!("chat-working-editing", target = target),
        ToolKind::Move => fl!("chat-working-moving", target = target),
        ToolKind::Read => fl!("chat-working-reading", target = target),
        ToolKind::Search => fl!("chat-working-searching", target = target),
        _ => step_title(&tool.title),
    }
}

/// A step as the open list shows it: its title, with the files it touches
/// unless the title names them. An agent's text is untrusted and unbounded,
/// so each line is cleaned on its own, which keeps the line breaks, and the
/// title is cut before anything is done with it.
fn step_text(tool: &Tool) -> String {
    let title = plain(&tool.title);
    let title = title
        .char_indices()
        .nth(MAX_STEP_TEXT)
        .map_or(title, |(end, _)| &title[..end]);
    let mut lines: Vec<String> = title
        .lines()
        .map(|line| safe_text(line, MAX_STEP_TEXT))
        .collect();
    if lines.iter().all(String::is_empty) {
        lines = vec![fl!("chat-working")];
    }
    if tool.kind != ToolKind::Execute {
        lines.extend(
            tool.locations
                .iter()
                .take(MAX_STEP_LOCATIONS)
                .filter(|location| !title.contains(short_path(location)))
                .map(|location| safe_text(location, MAX_LINE)),
        );
    }
    lines.join("\n")
}

/// A step's title as one plain, clean line.
fn step_title(title: &str) -> String {
    let line = safe_text(plain(title.lines().next().unwrap_or_default()), MAX_LINE);
    if line.is_empty() {
        fl!("chat-working")
    } else {
        line
    }
}

/// `text` without the code ticks an agent often wraps a command in.
fn plain(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix('`')
        .and_then(|inner| inner.strip_suffix('`'))
        .filter(|inner| !inner.contains('`'))
        .unwrap_or(text)
}

/// The row's mark: a dot in the phase's color that pulses while the agent
/// works, or a still icon while it waits.
pub(super) fn sign(phase: Phase, look: &Look<'_>) -> AnyElement {
    let color = phase.color(look);
    if phase == Phase::Waiting {
        return svg()
            .flex_none()
            .path(WAITING_ICON)
            .size(px((look.text * 0.85).max(11.)))
            .text_color(rgb(color))
            .into_any_element();
    }
    pulse(color, look).into_any_element()
}

/// A dot of `color` that pulses while something runs.
pub(super) fn pulse(color: u32, look: &Look<'_>) -> impl IntoElement {
    div()
        .flex_none()
        .size(px((look.text * 0.42).max(6.)))
        .rounded_full()
        .bg(rgb(color))
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
                .child(sign(working.phase, look))
                .child(if working.phase == Phase::Waiting {
                    div()
                        .debug_selector(|| "chat-waiting".into())
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(super::text_ink(
                            working.phase.color(look),
                            theme.background,
                        )))
                        .child(working.doing.clone())
                } else {
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.subtext()))
                        .child(fl!("chat-working-doing", doing = working.doing.as_str()))
                })
                .children(working.step.clone().map(|step| {
                    div()
                        .flex_none()
                        .text_color(rgb(theme.muted))
                        .child(format!("\u{b7} {step}"))
                }))
                .when(working.more > 0, |row| {
                    row.child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.muted))
                            .child(fl!("chat-working-more", count = working.more)),
                    )
                })
                .child(
                    svg()
                        .flex_none()
                        .size(px(10.))
                        .path(if working.open {
                            "icons/chevron-down.svg"
                        } else {
                            "icons/chevron-right.svg"
                        })
                        .text_color(rgb(theme.muted)),
                )
                .children(working.elapsed.clone().map(|elapsed| {
                    div()
                        .flex_none()
                        .ml_auto()
                        .text_color(rgb(theme.muted))
                        .child(elapsed)
                })),
        })
}

impl SheprdWindow {
    /// The steps running now, in full, opened from the working row; it
    /// grows in and shrinks out, from where it is when turned around. It
    /// stays open between steps, saying what the agent does instead, so it
    /// does not jump as each one ends.
    pub(super) fn render_chat_working_steps(&self, chat: &ChatPane) -> Option<AnyElement> {
        let now = motion::now();
        let shown = chat.steps.shown(now);
        if shown <= 0. {
            return None;
        }
        let transcript = chat.session.transcript();
        let now_ms = crate::chat::now_ms();
        let working = Working::of(transcript, now_ms)?;
        let look = self.chat_look();
        let theme = look.theme;
        let size = (look.text - 1.).max(10.);
        let height = size * 1.6 * LINES;
        let Steps { hidden, mut list } = Steps::of(transcript, now_ms);
        if list.is_empty() {
            list.push(Step {
                elapsed: None,
                text: working.doing,
            });
        }
        let rows = list.into_iter().enumerate().map(|(index, step)| {
            div()
                .debug_selector(move || format!("chat-step-{index}"))
                .flex()
                .items_start()
                .gap(px(10.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(rgb(theme.foreground))
                        .child(step.text),
                )
                .children(step.elapsed.map(|elapsed| {
                    div()
                        .flex_none()
                        .text_color(rgb(theme.muted))
                        .child(elapsed)
                }))
        });
        Some(
            div()
                .overflow_hidden()
                .max_h(px((height + 8.) * shown))
                .opacity(shown)
                .child(
                    div()
                        .id("chat-steps")
                        .debug_selector(|| "chat-steps".into())
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .mx(px(16.))
                        .py(px(4.))
                        .max_h(px(height + 8.))
                        .overflow_y_scroll()
                        .border_t_1()
                        .border_color(rgb(theme.active))
                        .text_size(px(size))
                        .children(rows)
                        .when(hidden > 0, |list| {
                            list.child(
                                div()
                                    .debug_selector(|| "chat-steps-hidden".into())
                                    .text_color(rgb(theme.muted))
                                    .child(fl!("chat-working-hidden", count = hidden)),
                            )
                        }),
                )
                .when(chat.steps.moving(now), |frame| {
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
