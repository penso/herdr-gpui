//! Sub-agents and background shells, as the bridges tag the tool calls that
//! start them and the calls a sub-agent makes.
//!
//! A bridge puts a `herdr` object in a call's or update's `_meta`:
//! `{"parent": "<sub-agent call id>"}` on a sub-agent's own steps, and
//! `{"activity": {"kind": "shell", "state": "running"}}` on the call that
//! started a sub-agent or background shell, with a later update saying how
//! it ended. Agents speaking ACP directly never send it; their calls stay
//! ordinary cards.

use acp::v1 as wire;
use herdr_chat_protocol::{
    Activity, ActivityKind, ActivityState, Change, MAX_ACTIVITIES, MAX_ACTIVITY_TEXT, Tool,
    ToolKind, Transcript,
};
use serde::{Deserialize, Serialize};

/// The `_meta` key the bridges' tags live under.
const KEY: &str = "herdr";

/// What a bridge says about a call beyond ACP.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Tag {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) activity: Option<Report>,
    /// The sub-agent call this call is a step of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parent: Option<String>,
}

/// A call's sub-agent or background shell, as of this message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Report {
    /// What it is doing now, such as a shell's latest status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) detail: Option<String>,
    pub(crate) kind: ActivityKind,
    /// Its label when the call's title is not, such as a shell's command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) label: Option<String>,
    #[serde(default)]
    pub(crate) state: ActivityState,
}

impl Report {
    pub(crate) fn new(kind: ActivityKind, state: ActivityState) -> Self {
        Self {
            detail: None,
            kind,
            label: None,
            state,
        }
    }
}

impl Tag {
    pub(crate) fn activity(report: Report) -> Self {
        Self {
            activity: Some(report),
            parent: None,
        }
    }

    /// This tag, or `other` when this one says nothing.
    pub(crate) fn or_else(self, other: impl FnOnce() -> Self) -> Self {
        if self == Self::default() {
            other()
        } else {
            self
        }
    }

    /// The tag in a call's `_meta`; a malformed one is no tag.
    pub(crate) fn read(meta: Option<&wire::Meta>) -> Self {
        meta.and_then(|meta| meta.get(KEY))
            .and_then(|tag| serde_json::from_value(tag.clone()).ok())
            .unwrap_or_default()
    }

    /// The `_meta` that carries this tag.
    pub(crate) fn meta(&self) -> wire::Meta {
        let mut meta = wire::Meta::new();
        if let Ok(tag) = serde_json::to_value(self) {
            meta.insert(KEY.to_owned(), tag);
        }
        meta
    }

    /// Marks the call itself: a sub-agent's kind, a step's parent.
    pub(crate) fn mark(&self, tool: &mut Tool) {
        if self
            .activity
            .as_ref()
            .is_some_and(|report| report.kind == ActivityKind::Subagent)
        {
            tool.kind = ToolKind::Subagent;
        }
        if let Some(parent) = &self.parent {
            tool.parent = Some(clip(parent));
        }
    }
}

/// The change that records `report` for the call `call_id` titled `title`,
/// or `None` when nothing it says is new. An activity that ended does not
/// run again, one first heard of as ended was never shown running and is not
/// recorded, and a new one waits for room rather than failing.
pub(crate) fn record(
    transcript: &Transcript,
    call_id: &str,
    title: &str,
    report: Report,
    now: u64,
) -> Option<Change> {
    if call_id.len() > MAX_ACTIVITY_TEXT {
        return None;
    }
    let known = transcript.activity(call_id);
    match known {
        Some(known) if !known.is_running() => return None,
        None if report.state != ActivityState::Running => return None,
        _ => {}
    }
    if known.is_none()
        && transcript.activities.len() >= MAX_ACTIVITIES
        && transcript.activities.iter().all(Activity::is_running)
    {
        return None;
    }
    let running = report.state == ActivityState::Running;
    let activity = Activity {
        call_id: call_id.to_owned(),
        detail: report
            .detail
            .as_deref()
            .map(clip)
            .or_else(|| known.and_then(|known| known.detail.clone())),
        finished_ms: (!running).then_some(now),
        kind: report.kind,
        label: report.label.as_deref().map_or_else(
            || known.map_or_else(|| clip(title), |known| known.label.clone()),
            clip,
        ),
        started_ms: known.map_or(now, |known| known.started_ms),
        state: report.state,
    };
    (Some(&activity) != known).then_some(Change::Activity { activity })
}

/// The change that shows a running sub-agent's latest step.
pub(crate) fn step(transcript: &Transcript, parent: &str, title: &str) -> Option<Change> {
    let known = transcript
        .activity(parent)
        .filter(|known| known.is_running())?;
    let detail = clip(title);
    if known.detail.as_ref() == Some(&detail) {
        return None;
    }
    Some(Change::Activity {
        activity: Activity {
            detail: Some(detail),
            ..known.clone()
        },
    })
}

/// Every activity still running, marked killed: the agent that ran them
/// is gone.
pub(crate) fn kill_all(transcript: &Transcript, now: u64) -> Vec<Change> {
    transcript
        .activities
        .iter()
        .filter(|activity| activity.is_running())
        .map(|activity| Change::Activity {
            activity: Activity {
                finished_ms: Some(now),
                state: ActivityState::Killed,
                ..activity.clone()
            },
        })
        .collect()
}

/// `text` cut to [`MAX_ACTIVITY_TEXT`] bytes on a character boundary, on
/// one line.
pub(crate) fn clip(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    let mut end = line.len().min(MAX_ACTIVITY_TEXT);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    line[..end].to_owned()
}
