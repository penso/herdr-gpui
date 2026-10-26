#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

mod connect;
mod mirror;
mod transcripts;

fn args(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

fn run(list: &[&str]) -> Options {
    match parse(args(list)).unwrap() {
        Parsed::Run(options) => options,
        other => panic!("expected options, got {other:?}"),
    }
}

#[test]
fn a_builtin_agent_with_its_own_arguments() {
    let options = run(&["--cwd", "/tmp", "claude", "--", "--model", "opus"]);
    assert_eq!(
        options.agent,
        AgentChoice::Builtin {
            agent: Agent::Claude,
            args: args(&["--model", "opus"]),
        }
    );
    assert_eq!(options.cwd, Some(PathBuf::from("/tmp")));
    assert!(options.attach);
    assert!(!options.headless);
}

#[test]
fn a_custom_command_after_the_separator() {
    let options = run(&[
        "--headless",
        "--no-attach",
        "--name",
        "Goose",
        "--",
        "goose",
        "acp",
    ]);
    assert_eq!(
        options.agent,
        AgentChoice::Command {
            program: OsString::from("goose"),
            args: args(&["acp"]),
        }
    );
    assert_eq!(options.name.as_deref(), Some("Goose"));
    assert!(options.headless);
    assert!(!options.attach);
}

#[test]
fn acp_asks_for_the_adapter() {
    assert!(!run(&["claude"]).adapter);
    assert!(run(&["--acp", "claude"]).adapter);
    assert!(run(&["claude", "--acp"]).adapter);
}

#[test]
fn load_takes_a_session_id() {
    assert_eq!(
        run(&["codex", "--load", "abc"]).load.as_deref(),
        Some("abc")
    );
}

#[test]
fn help_and_version_win_over_everything_else() {
    assert_eq!(parse(args(&["claude", "--help"])).unwrap(), Parsed::Help);
    assert_eq!(parse(args(&["-V"])).unwrap(), Parsed::Version);
}

#[test]
fn mistakes_are_named() {
    assert_eq!(parse(args(&[])), Err(Error::MissingAgent));
    assert_eq!(
        parse(args(&["cursor"])),
        Err(Error::UnknownAgent("cursor".to_owned()))
    );
    assert_eq!(
        parse(args(&["--frob"])),
        Err(Error::UnknownOption(OsString::from("--frob")))
    );
    assert_eq!(parse(args(&["--cwd"])), Err(Error::MissingValue("--cwd")));
    assert_eq!(parse(args(&["--"])), Err(Error::EmptyCommand));
    assert_eq!(
        parse(args(&["claude", "codex"])),
        Err(Error::UnknownOption(OsString::from("codex")))
    );
    assert!(Error::MissingAgent.to_string().contains("claude"));
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_are_accepted_where_paths_go() {
    use std::os::unix::ffi::OsStringExt;
    let odd = OsString::from_vec(vec![b'/', 0xff]);
    let options = match parse(vec![OsString::from("--cwd"), odd.clone(), "gemini".into()]).unwrap()
    {
        Parsed::Run(options) => options,
        other => panic!("{other:?}"),
    };
    assert_eq!(options.cwd, Some(PathBuf::from(odd.clone())));
    assert_eq!(
        parse(vec![OsString::from("--load"), odd]),
        Err(Error::NotUtf8("--load"))
    );
}
