//! `automation approve FILE [--host ID]` parses without touching the file;
//! the client resolves it.
use super::*;
use control::{AutomationCommand, ControlCommand};

fn command(args: &[&str]) -> Result<ControlCommand, CliError> {
    match LaunchOptions::parse(args.iter().copied()) {
        Ok(LaunchOptions {
            mode: LaunchMode::Control(invocation),
            ..
        }) => Ok(invocation.command),
        Ok(other) => panic!("automation parsed as {:?}", other.mode),
        Err(error) => Err(error),
    }
}

#[test]
fn approve_names_a_file_and_maybe_a_host() {
    assert_eq!(
        command(&["automation", "approve", "nightly.luau"]).unwrap(),
        ControlCommand::Automation(AutomationCommand::Approve {
            file: "nightly.luau".into(),
            host: None,
        })
    );
    assert_eq!(
        command(&[
            "automation",
            "approve",
            "--host",
            "buildbox",
            "/srv/.herdr/automations/a.luau",
            "--json",
        ])
        .unwrap(),
        ControlCommand::Automation(AutomationCommand::Approve {
            file: "/srv/.herdr/automations/a.luau".into(),
            host: Some("buildbox".into()),
        })
    );
}

#[test]
fn approve_refuses_a_missing_file_or_an_unknown_verb() {
    assert!(command(&["automation", "approve"]).is_err());
    assert!(command(&["automation"]).is_err());
    assert!(command(&["automation", "run", "a.luau"]).is_err());
    assert!(command(&["automation", "approve", "a.luau", "b.luau"]).is_err());
}

#[test]
fn approve_has_its_own_help() {
    assert_eq!(
        command(&["automation", "--help"]).unwrap(),
        ControlCommand::Help(Some("automation"))
    );
}
