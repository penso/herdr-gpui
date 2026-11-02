//! What `claude-agent-acp` says about Claude Code's calls in `_meta`, read
//! as the native bridge's tags: the sub-agent a call is a step of, and
//! sub-agents that run in the foreground.
//!
//! The adapter drops Claude Code's task notifications, so the end of a
//! background shell or a background sub-agent cannot be known on this path;
//! those stay ordinary cards rather than activities that never end.

use acp::v1 as wire;
use herdr_chat_protocol::{ActivityKind, ActivityState};
use serde_json::Value;

use super::activity::{Report, Tag};

/// The tag the adapter's `_meta.claudeCode` amounts to, given the call's
/// input when it has one and its status when it has one.
pub(super) fn tag(
    meta: Option<&wire::Meta>,
    input: Option<&Value>,
    status: Option<wire::ToolCallStatus>,
) -> Tag {
    let Some(code) = meta.and_then(|meta| meta.get("claudeCode")) else {
        return Tag::default();
    };
    let parent = code
        .get("parentToolUseId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let subagent = matches!(
        code.get("toolName").and_then(Value::as_str),
        Some("Agent" | "Task")
    );
    let background = input
        .and_then(|input| input.get("run_in_background"))
        .and_then(Value::as_bool)
        == Some(true)
        || code
            .pointer("/toolResponse/isAsync")
            .and_then(Value::as_bool)
            == Some(true);
    let activity = (subagent && !background).then(|| {
        let state = match status {
            Some(wire::ToolCallStatus::Completed) => ActivityState::Finished,
            Some(wire::ToolCallStatus::Failed) => ActivityState::Failed,
            _ => ActivityState::Running,
        };
        Report::new(ActivityKind::Subagent, state)
    });
    Tag { activity, parent }
}
