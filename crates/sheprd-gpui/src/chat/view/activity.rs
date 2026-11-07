//! Where a chat stands, as its header says it: a word and a mark for each
//! state, so a failed or cancelled turn does not read as a quiet agent and
//! an agent waiting on the user does not read as a busy one.
//!
//! Whether the agent runs comes from its program, or for an observed pane
//! from the daemon, and never from the transcript's entries: those only say
//! how the last turn ended once the agent is idle.

use super::{ChatInput, ChatPane, Hues, status_text};
use crate::{SheprdWindow, config::Theme, i18n::fl};
use gpui::{prelude::*, *};
use sheprd_chat_protocol::{EntryKind, Status, Transcript, TurnEnd};

/// Entries looked through for the last turn's end, as this runs each frame.
/// Only notices may follow one, so it is rarely more than the first.
const RECENT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum State {
    Completed,
    Exited,
    Failed,
    Idle,
    Starting,
    Stopped,
    Waiting,
    Working,
}

impl State {
    /// How `transcript` stands, given whether the agent `needs_input`. The
    /// agent having failed or exited outranks anything it last asked.
    pub(super) fn of(transcript: &Transcript, needs_input: bool) -> Self {
        match transcript.status {
            Status::Failed { .. } => Self::Failed,
            Status::Exited { .. } => Self::Exited,
            _ if needs_input => Self::Waiting,
            Status::Starting => Self::Starting,
            Status::Working => Self::Working,
            Status::Idle => Self::after_turn(transcript),
        }
    }

    /// How the last turn ended, while nothing has followed it. A reply that
    /// came with no turn's end says nothing, so the agent is just idle.
    fn after_turn(transcript: &Transcript) -> Self {
        let last = transcript
            .entries
            .iter()
            .rev()
            .take(RECENT)
            .find(|entry| !matches!(entry.kind, EntryKind::Notice { .. }));
        let Some(EntryKind::Turn(summary)) = last.map(|entry| &entry.kind) else {
            return Self::Idle;
        };
        match summary.end {
            TurnEnd::Done => Self::Completed,
            TurnEnd::Failed => Self::Failed,
            TurnEnd::Cancelled | TurnEnd::Limit | TurnEnd::Refused => Self::Stopped,
            TurnEnd::Other => Self::Idle,
        }
    }

    /// The header's word for this state, with the running turn's time.
    pub(super) fn text(self, transcript: &Transcript) -> String {
        match self {
            Self::Completed => fl!("chat-activity-completed"),
            Self::Failed => fl!("chat-status-failed"),
            Self::Stopped => fl!("chat-activity-stopped"),
            Self::Waiting => fl!("chat-needs-input"),
            Self::Exited | Self::Idle | Self::Starting | Self::Working => status_text(transcript),
        }
    }

    /// The icon of a state that has one to tell it apart; the rest show a dot.
    fn icon(self) -> Option<&'static str> {
        match self {
            Self::Completed => Some("icons/check.svg"),
            Self::Exited | Self::Failed => Some("icons/error.svg"),
            Self::Stopped => Some("icons/x.svg"),
            Self::Waiting => Some(super::working::WAITING_ICON),
            Self::Idle | Self::Starting | Self::Working => None,
        }
    }

    fn color(self, theme: &Theme) -> u32 {
        let hues = Hues::of(theme);
        match self {
            Self::Completed | Self::Idle => hues.green,
            Self::Exited | Self::Failed => hues.red,
            Self::Starting => theme.muted,
            Self::Stopped => theme.subtext(),
            Self::Waiting => hues.yellow,
            Self::Working => hues.blue,
        }
    }

    /// The header's mark for this state.
    pub(super) fn mark(self, theme: &Theme) -> AnyElement {
        let color = rgb(self.color(theme));
        match self.icon() {
            Some(icon) => svg()
                .flex_none()
                .path(icon)
                .size(px(12.))
                .text_color(color)
                .into_any_element(),
            None => div()
                .flex_none()
                .size(px(7.))
                .rounded_full()
                .bg(color)
                .into_any_element(),
        }
    }
}

impl SheprdWindow {
    /// Whether the agent is waiting on the user. A chat's own program says
    /// so by its permission requests. An observed agent asks in its terminal,
    /// which only the daemon sees, and its log may hold requests long since
    /// answered.
    pub(super) fn chat_needs_input(&self, chat: &ChatPane) -> bool {
        match chat.input() {
            ChatInput::Composer => chat.session.transcript().is_blocked(),
            ChatInput::ReadOnly | ChatInput::Terminal => {
                self.live.agent_status(&chat.target.pane_id)
                    == Some(sheprd_client::protocol::AgentStatus::Blocked)
            }
        }
    }
}

#[cfg(test)]
mod tests;
