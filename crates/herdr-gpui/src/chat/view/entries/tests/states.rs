use super::*;

#[core::prelude::v1::test]
fn a_card_says_how_what_its_call_started_stands() {
    // A background shell's call is done, but the shell runs on.
    assert_eq!(
        tool_state(Some(ActivityState::Running), ToolStatus::Completed),
        ToolState::Running
    );
    assert_eq!(
        tool_state(Some(ActivityState::Killed), ToolStatus::Completed),
        ToolState::Stopped
    );
    assert_eq!(
        tool_state(Some(ActivityState::Failed), ToolStatus::Completed),
        ToolState::Failed
    );
    assert_eq!(
        tool_state(Some(ActivityState::Finished), ToolStatus::InProgress),
        ToolState::Done
    );
    // Without one, the call's own status.
    assert_eq!(tool_state(None, ToolStatus::Pending), ToolState::Pending);
    assert_eq!(tool_state(None, ToolStatus::InProgress), ToolState::Running);
    assert_eq!(tool_state(None, ToolStatus::Failed), ToolState::Failed);
}

#[core::prelude::v1::test]
fn a_sub_agent_counts_its_steps() {
    let plain = |text: String| text.replace(['\u{2068}', '\u{2069}'], "");
    assert_eq!(plain(steps_label(1)), "1 step");
    assert_eq!(plain(steps_label(12)), "12 steps");
    assert_eq!(tool_kind(ToolKind::Subagent), "Sub-agent");
}
