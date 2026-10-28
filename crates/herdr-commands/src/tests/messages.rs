use super::*;

/// Only the numbered forms of one command share a message, which tells
/// them apart by `$number`.
#[test]
fn each_command_has_its_own_message_but_numbered_ones_share_theirs() {
    let mut seen: std::collections::HashMap<&str, Command> = std::collections::HashMap::new();
    for info in COMMANDS {
        assert!(info.message.starts_with("commands-"), "{}", info.message);
        let Some(earlier) = seen.insert(info.message, info.command) else {
            continue;
        };
        assert!(
            earlier.number().is_some()
                && info.command.number().is_some()
                && std::mem::discriminant(&earlier) == std::mem::discriminant(&info.command),
            "{} names {earlier:?} and {:?}",
            info.message,
            info.command
        );
    }
}

#[test]
fn numbered_commands_report_their_number() {
    assert_eq!(Command::TabNumber(3).number(), Some(3));
    assert_eq!(Command::WorkspaceNumber(9).number(), Some(9));
    assert_eq!(Command::AgentNumber(1).number(), Some(1));
    assert_eq!(Command::Tab.number(), None);
}
