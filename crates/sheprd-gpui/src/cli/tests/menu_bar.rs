use super::*;
use control::{ControlCommand, Invocation, Switch, UiCommand};

fn command(args: &[&str]) -> ControlCommand {
    match LaunchOptions::parse(args.iter().copied()).unwrap().mode {
        LaunchMode::Control(Invocation { command, .. }) => command,
        mode => panic!("{args:?} parsed as {mode:?}"),
    }
}

#[test]
fn ui_menubar_takes_a_switch_under_either_spelling() {
    assert_eq!(
        command(&["ui", "menubar"]),
        ControlCommand::Ui(UiCommand::MenuBar(Switch::Toggle))
    );
    assert_eq!(
        command(&["ui", "menu-bar", "off"]),
        ControlCommand::Ui(UiCommand::MenuBar(Switch::Off))
    );
}
