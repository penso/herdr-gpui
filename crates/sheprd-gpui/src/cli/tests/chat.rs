use super::*;
use control::{ChatCommand, ControlCommand, Invocation, Switch};

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
fn chat_lists_by_default() {
    assert_eq!(
        command(&["chat"]),
        ControlCommand::Chat(ChatCommand::Status)
    );
    assert_eq!(
        command(&["chat", "status"]),
        ControlCommand::Chat(ChatCommand::Status)
    );
    let json = parse(&["chat", "--json", "--window", "7"]).unwrap();
    assert!(json.json);
    assert_eq!(json.window, Some(7));
}

#[test]
fn chat_shows_or_hides_a_pane() {
    for (verb, switch) in [
        ("show", Switch::On),
        ("hide", Switch::Off),
        ("toggle", Switch::Toggle),
    ] {
        assert_eq!(
            command(&["chat", verb]),
            ControlCommand::Chat(ChatCommand::View { switch, pane: None })
        );
    }
    assert_eq!(
        command(&["chat", "hide", "--pane", "w1:p2"]),
        ControlCommand::Chat(ChatCommand::View {
            switch: Switch::Off,
            pane: Some("w1:p2".into())
        })
    );
}

#[test]
fn chat_refuses_what_it_does_not_know() {
    assert!(parse(&["chat", "prompt", "hello"]).is_err());
    assert!(parse(&["chat", "show", "extra"]).is_err());
    assert!(matches!(
        command(&["chat", "--help"]),
        ControlCommand::Help(Some("chat"))
    ));
}

#[test]
fn chat_new_names_a_built_in_agent() {
    assert_eq!(
        command(&["chat", "new", "claude"]),
        ControlCommand::Chat(ChatCommand::New {
            agent: sheprd_chat_protocol::Agent::Claude,
            pane: None
        })
    );
    assert_eq!(
        command(&["chat", "new", "--pane", "w1:p2", "codex"]),
        ControlCommand::Chat(ChatCommand::New {
            agent: sheprd_chat_protocol::Agent::Codex,
            pane: Some("w1:p2".into())
        })
    );
    assert!(matches!(
        parse(&["chat", "new", "cursor"]),
        Err(CliError::InvalidValue("AGENT"))
    ));
    assert!(matches!(
        parse(&["chat", "new"]),
        Err(CliError::MissingArgument { .. })
    ));
    assert!(parse(&["chat", "new", "claude", "codex"]).is_err());
}
