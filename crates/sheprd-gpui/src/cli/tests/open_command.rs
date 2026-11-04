use super::*;
use control::{ControlCommand, Invocation};

#[test]
fn open_takes_one_folder_and_the_global_options() {
    let parse = |args: &[&str]| {
        LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
            LaunchMode::Control(invocation) => invocation,
            mode => panic!("{args:?} parsed as {mode:?}"),
        })
    };
    assert_eq!(
        parse(&["open", "/work/herdr", "--json", "--window", "4"]),
        Ok(Invocation {
            command: ControlCommand::Open {
                path: "/work/herdr".into(),
            },
            json: true,
            window: Some(4),
        })
    );
    assert_eq!(
        parse(&["open"]),
        Err(CliError::MissingArgument {
            command: "open",
            what: "DIR",
        })
    );
    assert!(parse(&["open", "a", "b"]).is_err());
}
