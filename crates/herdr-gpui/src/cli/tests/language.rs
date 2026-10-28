use super::*;
use control::{ControlCommand, LanguageCommand};
use herdr_i18n::Language;

fn parse(args: &[&str]) -> Result<ControlCommand, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation.command,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

#[test]
fn language_reads_lists_and_sets_the_setting() {
    for args in [&["language"][..], &["language", "status"]] {
        assert_eq!(
            parse(args).unwrap(),
            ControlCommand::Language(LanguageCommand::Status)
        );
    }
    assert_eq!(
        parse(&["language", "list", "--json"]).unwrap(),
        ControlCommand::Language(LanguageCommand::List)
    );
    assert_eq!(
        parse(&["language", "set", "system"]).unwrap(),
        ControlCommand::Language(LanguageCommand::Set(Language::System))
    );
    assert_eq!(
        parse(&["language", "set", "zh-Hant-TW"]).unwrap(),
        ControlCommand::Language(LanguageCommand::Set(Language::Locale(
            "zh-Hant-TW".parse().unwrap()
        )))
    );
}

#[test]
fn language_refuses_what_is_not_a_tag() {
    assert!(matches!(
        parse(&["language", "set", "not a tag"]),
        Err(CliError::InvalidValue(_))
    ));
    assert!(matches!(
        parse(&["language", "set"]),
        Err(CliError::MissingArgument { .. })
    ));
    assert!(parse(&["language", "unset"]).is_err());
}
