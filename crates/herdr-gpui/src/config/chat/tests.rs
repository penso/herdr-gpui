#![allow(clippy::unwrap_used)]

use super::*;

fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn the_field_reads_and_writes_name_value_pairs() {
    let parsed =
        parse_env(r#" GH_HOST=mycompany.ghe.com  NOTE="two words" PATHS="a\"b" EMPTY="" "#);
    assert_eq!(
        parsed,
        Some(env(&[
            ("EMPTY", ""),
            ("GH_HOST", "mycompany.ghe.com"),
            ("NOTE", "two words"),
            ("PATHS", "a\"b"),
        ]))
    );
    let text = env_text(&parsed.unwrap());
    assert_eq!(
        text,
        r#"EMPTY="" GH_HOST=mycompany.ghe.com NOTE="two words" PATHS="a\"b""#
    );
    assert_eq!(
        parse_env(&text).map(|env| env.len()),
        Some(4),
        "round trips"
    );
    assert_eq!(parse_env(""), Some(BTreeMap::new()));
}

#[test]
fn malformed_or_unsafe_pairs_are_refused() {
    for text in [
        "GH_HOST",
        "=value",
        "1ABC=x",
        "MY-VAR=x",
        "A=\"unclosed",
        "A=\"x\"y",
        "HERDR_ACP=/tmp/evil",
    ] {
        assert_eq!(parse_env(text), None, "{text}");
    }
    assert!(check_env(&env(&[("A", "x\0y")])).is_err());
    let many: BTreeMap<String, String> = (0..=MAX_ENV)
        .map(|i| (format!("V{i}"), String::new()))
        .collect();
    assert!(matches!(
        check_env(&many),
        Err(crate::chat::Error::EnvironmentSize)
    ));
    let large = env(&[("BIG", &"x".repeat(MAX_ENV_BYTES))]);
    assert!(matches!(
        check_env(&large),
        Err(crate::chat::Error::EnvironmentSize)
    ));
}

#[test]
fn the_layout_is_read_and_saved_by_name() {
    let config = crate::config::Config::parse("[chat]\nlayout = \"cards\"\n").unwrap();
    assert_eq!(config.chat.layout, ChatLayoutMode::Cards);
    assert!(crate::config::Config::parse("[chat]\nlayout = \"stack\"\n").is_err());
    // A retired layout a config may still name opens in its closest one.
    assert_eq!(
        crate::config::Config::parse("[chat]\nlayout = \"pill\"\n")
            .unwrap()
            .chat
            .layout,
        ChatLayoutMode::Bubbles
    );
    assert_eq!(
        crate::config::Config::parse("").unwrap().chat.layout,
        ChatLayoutMode::Timeline
    );

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.local.toml");
    std::fs::write(&path, "[chat.env]\nGH_HOST = \"ghe.example\"\n").unwrap();
    for &mode in ChatLayoutMode::ALL {
        crate::config::Config::save_preference_path(
            crate::config::preferences::Preference::ChatLayout(mode),
            &path,
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let config = crate::config::Config::parse(&text).unwrap();
        assert_eq!(config.chat.layout, mode);
        assert_eq!(config.chat.env.get("GH_HOST").unwrap(), "ghe.example");
    }
}

#[test]
fn chats_jump_to_new_output_unless_glide_is_on() {
    assert!(!crate::config::Config::parse("").unwrap().chat.glide);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.local.toml");
    std::fs::write(&path, "[chat]\nlayout = \"imessage\"\n").unwrap();
    for glide in [true, false] {
        crate::config::Config::save_preference_path(
            crate::config::preferences::Preference::ChatGlide(glide),
            &path,
        )
        .unwrap();
        let config =
            crate::config::Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(config.chat.glide, glide);
        assert_eq!(config.chat.layout, ChatLayoutMode::IMessage);
    }
}

mod launcher;
