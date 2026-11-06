use super::*;
use control::{CloudCommand, ControlCommand, ExecCommand, Invocation};

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

fn words(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn cloud_exec_takes_the_command_after_a_double_dash_word_for_word() {
    let invocation = parse(&[
        "cloud",
        "exec",
        "coder:w1",
        "--json",
        "--cwd",
        "~/code/app",
        "--",
        "git",
        "log",
        "--json",
        "--help",
    ])
    .unwrap();
    assert!(invocation.json);
    assert_eq!(
        invocation.command,
        ControlCommand::Cloud(CloudCommand::Exec(ExecCommand {
            device: "coder:w1".into(),
            command: words(&["git", "log", "--json", "--help"]),
            cwd: Some("~/code/app".into()),
            timeout: 120,
        }))
    );
    let timed = parse(&["cloud", "exec", "w1", "--timeout", "600", "--", "true"]).unwrap();
    assert!(matches!(
        timed.command,
        ControlCommand::Cloud(CloudCommand::Exec(ExecCommand { timeout: 600, .. }))
    ));
    assert!(!control::wants_json(&[
        "cloud".into(),
        "exec".into(),
        "w1".into(),
        "--".into(),
        "--json".into()
    ]));
}

#[test]
fn cloud_exec_needs_a_device_and_a_command() {
    let missing = CliError::MissingArgument {
        command: "cloud exec",
        what: "-- COMMAND",
    };
    assert_eq!(parse(&["cloud", "exec", "w1"]).unwrap_err(), missing);
    assert_eq!(parse(&["cloud", "exec", "w1", "--"]).unwrap_err(), missing);
    assert_eq!(
        parse(&["cloud", "exec", "--", "ls"]).unwrap_err(),
        CliError::MissingArgument {
            command: "cloud",
            what: "ID"
        }
    );
    assert_eq!(
        parse(&["cloud", "exec", "w1", "--timeout", "0", "--", "ls"]).unwrap_err(),
        CliError::InvalidValue("--timeout")
    );
    // A `--` any other command leaves unread is refused.
    assert!(parse(&["cloud", "list", "--", "x"]).is_err());
}
