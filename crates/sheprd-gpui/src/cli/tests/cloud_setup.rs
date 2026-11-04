use super::*;
use control::{CloudCommand, ControlCommand, Invocation};

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

#[test]
fn cloud_setup_names_one_machine() {
    assert_eq!(
        parse(&["cloud", "setup", "coder:w1"]).unwrap().command,
        ControlCommand::Cloud(CloudCommand::Setup("coder:w1".into()))
    );
    assert_eq!(
        parse(&["cloud", "setup"]).unwrap_err(),
        CliError::MissingArgument {
            command: "cloud",
            what: "ID"
        }
    );
}
