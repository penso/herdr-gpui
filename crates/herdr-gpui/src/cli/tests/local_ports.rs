//! `ports all` and `ports stop|pause|resume PORT`.
use super::*;
use control::{ControlCommand, PortsCommand};
use herdr_control_protocol::protocol::hosts::PortAction;

fn command(args: &[&str]) -> Result<ControlCommand, CliError> {
    match LaunchOptions::parse(args.iter().copied()) {
        Ok(LaunchOptions {
            mode: LaunchMode::Control(invocation),
            ..
        }) => Ok(invocation.command),
        Ok(other) => panic!("{args:?} parsed as {:?}", other.mode),
        Err(error) => Err(error),
    }
}

#[test]
fn ports_all_lists_this_computer() {
    assert_eq!(
        command(&["ports", "all", "--json"]).unwrap(),
        ControlCommand::Ports(PortsCommand::All)
    );
}

#[test]
fn signals_name_one_port() {
    for (verb, action) in [
        ("stop", PortAction::Stop),
        ("pause", PortAction::Pause),
        ("resume", PortAction::Resume),
    ] {
        assert_eq!(
            command(&["ports", verb, "3000"]).unwrap(),
            ControlCommand::Ports(PortsCommand::Signal(3000, action))
        );
        assert!(matches!(
            command(&["ports", verb]).unwrap_err(),
            CliError::MissingArgument { what: "PORT", .. }
        ));
        assert_eq!(
            command(&["ports", verb, "0"]).unwrap_err(),
            CliError::InvalidValue("PORT")
        );
        assert_eq!(
            command(&["ports", verb, "70000"]).unwrap_err(),
            CliError::InvalidValue("PORT")
        );
    }
    assert!(command(&["ports", "kill", "3000"]).is_err());
}

#[test]
fn ports_help_covers_the_new_verbs() {
    assert_eq!(
        command(&["ports", "stop", "--help"]).unwrap(),
        ControlCommand::Help(Some("ports"))
    );
}
