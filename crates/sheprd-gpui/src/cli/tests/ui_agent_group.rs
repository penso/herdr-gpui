use super::*;
use control::{ControlCommand, UiCommand};

fn command(args: &[&str]) -> ControlCommand {
    match LaunchOptions::parse(args.iter().copied()).unwrap().mode {
        LaunchMode::Control(invocation) => invocation.command,
        mode => panic!("{args:?} parsed as {mode:?}"),
    }
}

#[test]
fn ui_agent_group_lists_picks_or_clears() {
    for (args, expected) in [
        (&["ui", "agent-group"][..], UiCommand::AgentGroups),
        (
            &["ui", "agent-group", "demo"],
            UiCommand::AgentGroup(Some("demo".to_owned())),
        ),
        (&["ui", "agent-group", "all"], UiCommand::AgentGroup(None)),
    ] {
        assert_eq!(command(args), ControlCommand::Ui(expected), "{args:?}");
    }
}
