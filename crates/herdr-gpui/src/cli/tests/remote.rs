use super::*;
use control::{ControlCommand, Invocation, RemoteCommand};

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

fn command(args: &[&str]) -> ControlCommand {
    parse(args).unwrap().command
}

#[test]
fn remote_reports_status_by_default() {
    for args in [&["remote"][..], &["remote", "status"]] {
        assert_eq!(command(args), ControlCommand::Remote(RemoteCommand::Status));
    }
    let json = parse(&["remote", "status", "--json", "--window", "3"]).unwrap();
    assert!(json.json);
    assert_eq!(json.window, Some(3));
}

#[test]
fn remote_pairs_lists_and_revokes() {
    assert_eq!(
        command(&["remote", "pair"]),
        ControlCommand::Remote(RemoteCommand::Pair)
    );
    for verb in ["devices", "list"] {
        assert_eq!(
            command(&["remote", verb, "--json"]),
            ControlCommand::Remote(RemoteCommand::Devices)
        );
    }
    assert_eq!(
        command(&["remote", "revoke", "d1"]),
        ControlCommand::Remote(RemoteCommand::Revoke {
            device: "d1".into()
        })
    );
    assert_eq!(
        command(&["remote", "revoke", "Fabien's", "iPhone"]),
        ControlCommand::Remote(RemoteCommand::Revoke {
            device: "Fabien's iPhone".into()
        })
    );
}

#[test]
fn remote_refuses_what_it_does_not_do() {
    assert!(matches!(
        parse(&["remote", "revoke"]),
        Err(CliError::MissingArgument { .. })
    ));
    // Approving stays in the dialog: there is no command for it.
    assert!(parse(&["remote", "approve", "p1"]).is_err());
    assert!(parse(&["remote", "pair", "extra"]).is_err());
    assert!(parse(&["remote", "status", "now"]).is_err());
    assert!(matches!(
        command(&["remote", "--help"]),
        ControlCommand::Help(Some("remote"))
    ));
}
