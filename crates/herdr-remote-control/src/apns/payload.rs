//! The Live Activity payloads APNs delivers to the companion app. The keys
//! of `attributes` and `content-state` are the Swift
//! `ChatActivityAttributes` and its `ContentState` in
//! `ios/Shared/ChatActivityAttributes.swift`, and
//! `ios/HerdrRemoteCore/Tests/HerdrRemoteCoreTests/Fixtures/` holds payloads
//! both sides test against.
//!
//! ```json
//! { "aps": {
//!     "timestamp": 1800000000, "event": "start",
//!     "attributes-type": "ChatActivityAttributes",
//!     "attributes": { "agent": "Claude", "chatTitle": "remote plan",
//!                     "computer": "studio", "computerId": "AB12…(64 hex)",
//!                     "pane": "w1-p3",
//!                     "workspace": "herdr-gpui" },
//!     "content-state": { "phase": "working", "step": "Reading lib.rs",
//!                        "startedAt": 1799999990, "cost": "$0.34",
//!                        "planDone": 2, "planTotal": 4,
//!                        "working": 1, "waiting": 0 },
//!     "alert": { "title": "Claude is working", "body": "herdr-gpui · remote plan" } } }
//! ```
//!
//! `startedAt` is seconds since the Unix epoch: the Swift side decodes it
//! itself rather than through `JSONDecoder`'s date strategy, which ActivityKit
//! does not let an app choose. `question` and `summary` appear in the
//! question and finished phases; every optional key may be absent.

use super::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// The largest payload APNs accepts for a Live Activity.
pub(crate) const MAX_PAYLOAD: usize = 4096;
/// The Swift type push-to-start names.
pub(crate) const ATTRIBUTES_TYPE: &str = "ChatActivityAttributes";

/// What a chat's turn is doing, as the activity shows it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    // keep-sorted start
    Failed,
    Finished,
    Question,
    Working,
    // keep-sorted end
}

/// The activity's fixed part, set when it starts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Attributes {
    /// The agent's display name, such as `Claude`.
    pub(crate) agent: String,
    pub(crate) chat_title: String,
    /// The computer's name, as pairing gave it to the app. Display only.
    pub(crate) computer: String,
    /// The computer's CA fingerprint, 64 uppercase hex digits without
    /// separators. The app routes the activity's link and push token by it,
    /// never by the name, which any paired computer could also choose.
    pub(crate) computer_id: String,
    pub(crate) pane: String,
    pub(crate) workspace: String,
}

/// The activity's changing part.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentState {
    /// The turn's cost so far, formatted, such as `$0.34`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cost: Option<String>,
    pub(crate) phase: Phase,
    /// Plan entries completed, when the agent keeps a plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) plan_done: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) plan_total: Option<u32>,
    /// The question waiting, in the question phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) question: Option<String>,
    /// When the turn started, in seconds since the Unix epoch.
    pub(crate) started_at: u64,
    /// The current tool call or plan step.
    pub(crate) step: String,
    /// How the turn ended, in the finished and failed phases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) summary: Option<String>,
    /// Questions waiting across this computer's chats, for the Dynamic
    /// Island; the sender fills it in.
    #[serde(default)]
    pub(crate) waiting: u32,
    /// Turns running across this computer's chats.
    #[serde(default)]
    pub(crate) working: u32,
}

/// A notification shown with a start or an update.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Alert {
    pub(crate) body: String,
    pub(crate) title: String,
}

/// What a push does to the chat's activity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Event {
    /// Push-to-start: the app may be closed, so the push carries the fixed
    /// part too, and an alert, which ActivityKit requires to start one.
    Start {
        alert: Alert,
        attributes: Attributes,
    },
    Update {
        alert: Option<Alert>,
    },
    /// The activity ends showing `state`, and leaves the lock screen at
    /// `dismissal_s` (seconds since the Unix epoch).
    End {
        dismissal_s: u64,
    },
}

impl Event {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Start { .. } => "start",
            Self::Update { .. } => "update",
            Self::End { .. } => "end",
        }
    }
}

/// One push for one chat's activity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Live {
    pub(crate) event: Event,
    pub(crate) pane: String,
    pub(crate) state: ContentState,
}

impl Live {
    /// Whether the push must reach the phone now: a question does, and
    /// counts against the app's budget; anything else can wait for a
    /// convenient moment.
    pub(crate) fn urgent(&self) -> bool {
        self.state.phase == Phase::Question
    }

    /// The JSON payload, sent at `timestamp_s` (seconds since the Unix
    /// epoch), which ActivityKit uses to drop out-of-order updates.
    pub(crate) fn payload(&self, timestamp_s: u64) -> Result<Vec<u8>> {
        let mut aps = Map::new();
        aps.insert("timestamp".into(), json!(timestamp_s));
        aps.insert("event".into(), json!(self.event.name()));
        aps.insert("content-state".into(), serde_json::to_value(&self.state)?);
        match &self.event {
            Event::Start { alert, attributes } => {
                aps.insert("attributes-type".into(), json!(ATTRIBUTES_TYPE));
                aps.insert("attributes".into(), serde_json::to_value(attributes)?);
                aps.insert("alert".into(), serde_json::to_value(alert)?);
            }
            Event::Update { alert: Some(alert) } => {
                aps.insert("alert".into(), serde_json::to_value(alert)?);
            }
            Event::Update { alert: None } => {}
            Event::End { dismissal_s } => {
                aps.insert("dismissal-date".into(), json!(dismissal_s));
            }
        }
        let mut root = Map::new();
        root.insert("aps".into(), Value::Object(aps));
        let bytes = serde_json::to_vec(&Value::Object(root))?;
        if bytes.len() > MAX_PAYLOAD {
            return Err(Error::PayloadTooLarge { len: bytes.len() });
        }
        Ok(bytes)
    }
}
