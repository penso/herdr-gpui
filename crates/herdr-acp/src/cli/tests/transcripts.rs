use super::*;

const SESSION: &str = "01234567-89ab-4cde-8123-456789abcdef";

#[test]
fn mirror_and_observe_name_the_same_agents() {
    for agent in ["gemini", "pi", "Codex", "--pane"] {
        assert_eq!(
            parse(args(&["mirror", agent, "/tmp/session.jsonl"])),
            Err(Error::TranscriptAgent)
        );
        assert_eq!(
            parse(args(&["observe", agent, SESSION, "123", "456"])),
            Err(Error::TranscriptAgent)
        );
    }
    assert_eq!(parse(args(&["observe"])), Err(Error::TranscriptAgent));
    let Ok(Parsed::Observe(options)) = parse(args(&["observe", "claude", SESSION, "1", "2"]))
    else {
        panic!("observe")
    };
    assert_eq!(options.agent, TranscriptAgent::Claude);
}

#[test]
fn help_is_accepted_anywhere_in_either_command() {
    for list in [
        &["mirror", "--help"][..],
        &["mirror", "codex", "-h"],
        &["mirror", "codex", "file", "--pane", "p1", "--help"],
        &["mirror", "gemini", "--help"],
        &["observe", "-h"],
        &["observe", "codex", "--help"],
        &["observe", "codex", SESSION, "123", "456", "--help"],
    ] {
        assert_eq!(parse(args(list)), Ok(Parsed::Help), "{list:?}");
    }
}
