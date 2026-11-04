//! What happened in the app, for callers that poll or wait on it. Events
//! are kept in a bounded log; a caller passes back the last sequence number
//! it saw and gets what came after.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventsOp {
    /// Events after `after`, waiting up to `wait_seconds` for the first.
    /// Without `after`, only events from now on.
    Poll {
        #[serde(default)]
        after: Option<u64>,
        /// The app run `after` counts in. Another run's numbers mean
        /// nothing here, so its caller gets every event this run kept.
        #[serde(default)]
        instance: Option<u64>,
        wait_seconds: u64,
        #[serde(default)]
        kinds: Vec<EventKind>,
    },
    /// Wait up to `wait_seconds` for `condition` to hold.
    Wait {
        condition: Condition,
        wait_seconds: u64,
    },
}

impl EventsOp {
    pub fn wait_seconds(&self) -> u64 {
        match self {
            Self::Poll { wait_seconds, .. } | Self::Wait { wait_seconds, .. } => *wait_seconds,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    WindowOpened,
    WindowClosed,
    WindowFocused,
    TabOpened,
    TabClosed,
    /// Notes were kept for a pane's `feedback`.
    Feedback,
    /// A worktree note changed.
    Note,
    /// A host's connection status changed.
    Host,
    /// A batch of cloud machines moved on: asked, running, done, cancelled.
    CloudBatch,
}

impl EventKind {
    pub const ALL: &[Self] = &[
        Self::WindowOpened,
        Self::WindowClosed,
        Self::WindowFocused,
        Self::TabOpened,
        Self::TabClosed,
        Self::Feedback,
        Self::Note,
        Self::Host,
        Self::CloudBatch,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::WindowOpened => "window_opened",
            Self::WindowClosed => "window_closed",
            Self::WindowFocused => "window_focused",
            Self::TabOpened => "tab_opened",
            Self::TabClosed => "tab_closed",
            Self::Feedback => "feedback",
            Self::Note => "note",
            Self::Host => "host",
            Self::CloudBatch => "cloud_batch",
        }
    }
}

impl std::str::FromStr for EventKind {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, ()> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.name() == text.replace('-', "_"))
            .ok_or(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "condition", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    /// The app has a window open.
    Window,
    /// The app tab `id` is gone.
    TabClosed { id: u64 },
    /// The window's selected host is online.
    Online,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    pub kind: EventKind,
    /// The window, tab, pane, or host the event is about.
    pub subject: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    /// `next` is the sequence number to pass back as `after`, with
    /// `instance`, the app run it counts in.
    Events {
        events: Vec<Event>,
        next: u64,
        #[serde(default)]
        instance: u64,
    },
    Waited {
        met: bool,
    },
}

/// Where a reader of events left off: a sequence number, and the app run it
/// counts in. Written `INSTANCE:SEQ`, or `SEQ` alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub instance: Option<u64>,
    pub seq: u64,
}

impl std::str::FromStr for Cursor {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, ()> {
        match text.split_once(':') {
            Some((instance, seq)) => Ok(Self {
                instance: Some(instance.parse().map_err(|_| ())?),
                seq: seq.parse().map_err(|_| ())?,
            }),
            None => Ok(Self {
                instance: None,
                seq: text.parse().map_err(|_| ())?,
            }),
        }
    }
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.instance {
            Some(instance) => write!(f, "{instance}:{}", self.seq),
            None => write!(f, "{}", self.seq),
        }
    }
}
