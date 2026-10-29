use super::*;

#[core::prelude::v1::test]
fn a_card_says_how_what_its_call_started_stands() {
    // A background shell's call is done, but the shell runs on.
    assert_eq!(
        tool_state(Some(ActivityState::Running), ToolStatus::Completed),
        "running"
    );
    assert_eq!(
        tool_state(Some(ActivityState::Killed), ToolStatus::Completed),
        "stopped"
    );
    assert_eq!(
        tool_state(Some(ActivityState::Failed), ToolStatus::Completed),
        "failed"
    );
    assert_eq!(
        tool_state(Some(ActivityState::Finished), ToolStatus::InProgress),
        "done"
    );
    // Without one, the call's own status.
    assert_eq!(tool_state(None, ToolStatus::Pending), "pending");
    assert_eq!(tool_state(None, ToolStatus::InProgress), "running");
    assert_eq!(tool_state(None, ToolStatus::Failed), "failed");
}

#[core::prelude::v1::test]
fn a_sub_agent_counts_its_steps() {
    assert_eq!(steps_label(1), "1 step");
    assert_eq!(steps_label(12), "12 steps");
    assert_eq!(tool_kind(ToolKind::Subagent), "Sub-agent");
}
