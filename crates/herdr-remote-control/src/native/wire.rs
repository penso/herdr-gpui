//! The messages of the live socket (`/api/v2/live`), one JSON object per
//! text frame, as `docs/ios-app-plan.md` specifies them.

use super::model::{ChatSnapshot, Home, Run};
use herdr_chat_protocol::{Change, Cursor};
use serde::{Deserialize, Serialize};

/// A view the phone follows. The same view subscribed again replaces the
/// first subscription.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "view")]
pub(crate) enum View {
    Home,
    Chat { pane: String },
    Term { pane: String },
}

/// What the phone sends.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub(crate) enum ClientMessage {
    Subscribe {
        #[serde(flatten)]
        view: View,
        /// The cursor of the chat transcript the phone holds already.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<Cursor>,
        /// The terminal the phone draws, for `term`: while it follows the
        /// view, Herdr sizes the pane's terminal to it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<TermSize>,
    },
    Unsubscribe {
        #[serde(flatten)]
        view: View,
    },
    /// Swiped a followed terminal: positive `lines` scroll up toward its
    /// history, negative down. Only a terminal followed at the phone's
    /// size scrolls.
    TermScroll { pane: String, lines: i32 },
}

/// The most lines one `term_scroll` moves; more is moved this far.
pub(crate) const MAX_SCROLL_LINES: i32 = 50;

/// A terminal size in cells, as the phone fits it to its screen.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub(crate) struct TermSize {
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

impl TermSize {
    /// The columns a phone may ask for; narrower is unreadable as a
    /// terminal, wider is no phone.
    pub(crate) const COLS: std::ops::RangeInclusive<u16> = 20..=400;
    pub(crate) const ROWS: std::ops::RangeInclusive<u16> = 5..=200;

    /// Whether Herdr may be asked to size a terminal to it.
    pub(crate) fn is_valid(self) -> bool {
        Self::COLS.contains(&self.cols) && Self::ROWS.contains(&self.rows)
    }
}

/// One change and the cursor it leads to.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Step {
    pub(crate) cursor: Cursor,
    pub(crate) change: Change,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ByeReason {
    /// The session ran out; the phone signs in again.
    Expired,
    /// The device was revoked or forgotten; the phone must pair again.
    Revoked,
}

/// What the daemon sends.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub(crate) enum ServerMessage {
    Home {
        home: Home,
    },
    ChatSnapshot(Box<ChatSnapshot>),
    /// The phone's cached transcript is current; nothing to send.
    ChatResumed {
        pane: String,
        cursor: Cursor,
        connected: bool,
    },
    ChatChanges {
        pane: String,
        changes: Vec<Step>,
    },
    ChatConnection {
        pane: String,
        connected: bool,
    },
    TermScreen {
        pane: String,
        columns: usize,
        rows: Vec<Vec<Run>>,
    },
    /// The view lagged or went away: fetch it again.
    Reload {
        #[serde(flatten)]
        view: View,
    },
    Error {
        message: String,
    },
    Bye {
        reason: ByeReason,
    },
}
