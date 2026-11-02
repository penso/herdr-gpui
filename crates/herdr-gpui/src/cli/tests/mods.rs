use super::*;
use control::{ControlCommand, Invocation, ModCommand};

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
fn mod_lists_by_default_with_the_shared_options() {
    assert_eq!(command(&["mod"]), ControlCommand::Mod(ModCommand::List));
    assert_eq!(
        command(&["mod", "list"]),
        ControlCommand::Mod(ModCommand::List)
    );
    let json = parse(&["mod", "--json", "--window", "3"]).unwrap();
    assert!(json.json);
    assert_eq!(json.window, Some(3));
}

#[test]
fn mod_reload_logs_and_check_take_their_arguments() {
    assert_eq!(
        command(&["mod", "reload", "agents"]),
        ControlCommand::Mod(ModCommand::Reload {
            name: "agents".into()
        })
    );
    assert_eq!(
        command(&["mod", "logs", "agents"]),
        ControlCommand::Mod(ModCommand::Logs {
            name: "agents".into(),
            lines: 50
        })
    );
    assert_eq!(
        command(&["mod", "logs", "agents", "--lines", "10"]),
        ControlCommand::Mod(ModCommand::Logs {
            name: "agents".into(),
            lines: 10
        })
    );
    assert_eq!(
        command(&["mod", "open", "dashboard"]),
        ControlCommand::Mod(ModCommand::Open {
            name: "dashboard".into()
        })
    );
    assert_eq!(
        command(&["mod", "check", "/tmp/agents"]),
        ControlCommand::Mod(ModCommand::Check {
            dir: "/tmp/agents".into()
        })
    );
}

#[test]
fn mod_refuses_bad_arguments() {
    assert!(parse(&["mod", "reload"]).is_err());
    assert!(parse(&["mod", "check"]).is_err());
    assert!(parse(&["mod", "open"]).is_err());
    assert!(parse(&["mod", "logs", "agents", "--lines", "500"]).is_err());
    assert!(parse(&["mod", "remove", "agents"]).is_err());
}

#[test]
fn mod_panel_reports_or_switches_the_panel() {
    use control::Switch;
    assert_eq!(
        command(&["mod", "panel"]),
        ControlCommand::Mod(ModCommand::Panel { switch: None })
    );
    for (word, switch) in [
        ("show", Switch::On),
        ("hide", Switch::Off),
        ("toggle", Switch::Toggle),
    ] {
        assert_eq!(
            command(&["mod", "panel", word]),
            ControlCommand::Mod(ModCommand::Panel {
                switch: Some(switch)
            })
        );
    }
    assert!(parse(&["mod", "panel", "sideways"]).is_err());
}
