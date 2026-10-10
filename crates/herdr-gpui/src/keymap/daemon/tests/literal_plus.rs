use super::*;

#[test]
fn a_literal_plus_follows_the_final_separator() {
    for (herdr, gpui) in [
        ("+", "+"),
        (" + ", "+"),
        ("plus", "+"),
        ("ctrl++", "ctrl-+"),
        ("ctrl+plus", "ctrl-+"),
        ("Control + alt ++", "ctrl-alt-+"),
        ("shift++", "shift-+"),
        ("super++", "cmd-+"),
    ] {
        assert_eq!(keystroke(herdr), Some(parsed(gpui)), "{herdr}");
    }
}

#[test]
fn a_literal_plus_rejects_missing_or_extra_key_tokens() {
    // Herdr's own rejections, plus `hyper`, which GPUI cannot type.
    for invalid in [
        "",
        "ctrl+",
        "++",
        "ctrl+++",
        "ctrl++x",
        "x++",
        "unknown++",
        "hyper++",
        "ctrl+hyper++",
    ] {
        assert_eq!(keystroke(invalid), None, "{invalid}");
    }
    assert!(triggers("prefix+++").is_empty());
    assert_eq!(triggers("prefix++"), [(None, prefixed("+"))]);
    assert_eq!(
        triggers("ctrl+alt++"),
        [(None, Trigger::Direct(parsed("ctrl-alt-+")))]
    );
}

#[test]
fn local_keys_bind_a_plus_prefix_and_plus_chords() {
    let keys = keys(
        r#"
            [keys]
            prefix = "ctrl++"
            new_tab = ["prefix++", "alt++"]
            "#,
    );
    assert_eq!(keys.prefixes, [parsed("ctrl-+")]);
    assert_eq!(
        bound(&keys, Command::Tab),
        [prefixed("+"), Trigger::Direct(parsed("alt-+"))]
    );
}

#[test]
fn plus_prefix_lists_keep_each_key_once() {
    let prefixes = |value: &str| keys(&format!("[keys]\nprefix = {value}")).prefixes;
    assert_eq!(
        prefixes(r#"["ctrl++", "alt++", "ctrl+plus", "ctrl+++"]"#),
        [parsed("ctrl-+"), parsed("alt-+")]
    );
    // Nothing usable keeps the default rather than an empty-key prefix.
    assert_eq!(prefixes(r#"["++", "x++"]"#), [parsed("ctrl-b")]);
}

#[test]
fn a_server_profile_keeps_a_plus_prefix() {
    // `format_key_combo` publishes `ctrl+plus` as `ctrl++`.
    let profile = r#"
            [keys]
            prefix = "ctrl++"
            extra_prefixes = ["alt++"]
            new_tab = ["prefix++"]
            "#;
    let keys = DaemonKeys::from_profile(Some(profile)).unwrap();
    assert_eq!(keys.prefixes, [parsed("ctrl-+"), parsed("alt-+")]);
    assert_eq!(bound(&keys, Command::Tab), [prefixed("+")]);
}
