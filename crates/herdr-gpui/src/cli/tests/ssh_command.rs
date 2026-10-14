use super::*;
use control::{ControlCommand, HostCommand, Invocation};

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

#[test]
fn host_ssh_command_honors_json_and_window_and_refuses_bad_input() {
    let invocation = parse(&["host", "--json", "ssh-command", "devbox", "--window", "7"]).unwrap();
    assert_eq!(
        invocation,
        Invocation {
            command: ControlCommand::Host(HostCommand::SshCommand("devbox".into())),
            json: true,
            window: Some(7),
        }
    );
    // The command refuses to act without a host, or with an unknown verb,
    // before starting GPUI.
    assert_eq!(
        parse(&["host", "ssh-command"]).unwrap_err(),
        CliError::MissingArgument {
            command: "host",
            what: "HOST"
        }
    );
    assert_eq!(
        parse(&["host", "explode"]).unwrap_err(),
        CliError::UnknownCommand {
            family: "host",
            command: "explode".into()
        }
    );
    // A trailing argument is never silently ignored.
    assert!(parse(&["host", "ssh-command", "devbox", "extra"]).is_err());
}
