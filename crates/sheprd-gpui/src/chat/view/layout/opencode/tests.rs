use super::*;
use core::prelude::v1::test;

#[test]
fn what_needs_attention_keeps_its_preview() {
    for state in [ToolState::Failed, ToolState::Running] {
        assert_eq!(OpenCode.tool_fold(state), ToolFold::Preview, "{state:?}");
    }
}

#[test]
fn what_finished_or_waits_is_one_line() {
    for state in [ToolState::Done, ToolState::Pending, ToolState::Stopped] {
        assert_eq!(OpenCode.tool_fold(state), ToolFold::Line, "{state:?}");
    }
}

#[test]
fn a_failed_bar_keeps_a_red_hue_on_a_dark_block() {
    let mut theme = Theme::default();
    theme.palette[9] = 0xfb4934;
    let fill = 0x3a3836;
    assert_eq!(failed(&theme, fill), 0xfb4934);
}
