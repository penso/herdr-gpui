#![allow(clippy::unwrap_used)]
use super::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn parse(text: &str) -> Result<SshCommand> {
    SshCommand::try_from(text)
}

#[test]
fn words_split_with_shell_quoting_and_normalize_to_one_spelling() {
    let command = parse("tailscale ssh").unwrap();
    assert_eq!(command.as_str(), "tailscale ssh");
    assert!(!command.is_default());

    // Two spellings of the same command are equal and hash alike.
    let quoted = parse("tailscale 'ssh'").unwrap();
    assert_eq!(command, quoted);

    // A backslash escape groups a word the same way quoting does.
    assert_eq!(
        parse(r"tailscale\ ssh").unwrap(),
        parse("'tailscale ssh'").unwrap()
    );

    // A quoted path with a space stays one word and round-trips.
    let path = parse("'/path/with space/ssh' -F /etc/ssh/config").unwrap();
    assert_eq!(path.as_str(), "'/path/with space/ssh' -F /etc/ssh/config");
    assert_eq!(parse(path.as_str()).unwrap(), path);

    // Quoting groups an argument carrying shell metacharacters; the canonical
    // form re-parses to the same words without expanding them.
    let literal = parse("ssh 'a;b $(touch nope) && c'").unwrap();
    assert_eq!(parse(literal.as_str()).unwrap(), literal);

    let mut a = DefaultHasher::new();
    command.hash(&mut a);
    let mut b = DefaultHasher::new();
    quoted.hash(&mut b);
    assert_eq!(a.finish(), b.finish());
}

#[test]
fn quoted_empty_arguments_are_allowed_but_an_empty_executable_is_not() {
    // A fixed empty argument is permitted by the contract.
    let with_empty = parse("ssh '' host").unwrap();
    assert_eq!(with_empty.as_str(), "ssh '' host");
    assert_eq!(parse(with_empty.as_str()).unwrap(), with_empty);

    // An empty executable is refused.
    assert!(matches!(
        parse("'' host"),
        Err(Error::InvalidSshCommand { .. })
    ));
}

#[test]
fn invalid_commands_are_typed_errors() {
    // Option-like executable.
    assert!(matches!(
        parse("-oBatchMode=yes"),
        Err(Error::InvalidSshCommand { .. })
    ));
    // Control characters.
    for text in [
        "ssh\nhost",
        "ssh\0host",
        "ssh\u{1b}host",
        "ssh\n",
        "\nssh",
        "# no executable",
    ] {
        assert!(matches!(parse(text), Err(Error::InvalidSshCommand { .. })));
    }
    // Incomplete quoting.
    for text in ["ssh 'host", "ssh \"host", "ssh host\\"] {
        assert!(matches!(parse(text), Err(Error::InvalidSshCommand { .. })));
    }
    // Word count bound.
    let too_many_words = std::iter::repeat_n("x", 65).collect::<Vec<_>>().join(" ");
    assert!(matches!(
        parse(&too_many_words),
        Err(Error::InvalidSshCommand { .. })
    ));
    // Byte bound.
    let too_long = format!("ssh {}", "x".repeat(4096));
    assert!(matches!(
        parse(&too_long),
        Err(Error::InvalidSshCommand { .. })
    ));
}

#[test]
fn serialization_is_a_validated_string() {
    let command = parse("tailscale ssh").unwrap();
    assert_eq!(
        serde_json::to_string(&command).unwrap(),
        "\"tailscale ssh\""
    );
    let decoded: SshCommand = serde_json::from_str("\"tailscale ssh\"").unwrap();
    assert_eq!(decoded, command);
    // The default serializes as the bare `ssh` word.
    assert_eq!(
        serde_json::to_string(&SshCommand::default()).unwrap(),
        "\"ssh\""
    );
    // Invalid strings are refused at the deserialization boundary.
    assert!(serde_json::from_str::<SshCommand>("\"-oProxyCommand=x\"").is_err());
    assert!(serde_json::from_str::<SshCommand>("\"ssh 'host\"").is_err());
}
