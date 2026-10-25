use super::*;
use crate::worktree_identity::{self, Worktrees};

#[test]
fn worktrees_are_marked_in_the_sidebar_in_the_themes_hue_by_default() -> anyhow::Result<()> {
    for config in [
        Config::default(),
        Config::parse("")?,
        Config::parse(DEFAULT_CONFIG)?,
        Config::parse("[worktrees]")?,
    ] {
        assert_eq!(config.worktrees, Worktrees::default());
        assert_eq!(config.worktrees.tint, None);
        assert!(config.worktrees.sidebar);
        assert!(!config.worktrees.terminal);
    }
    Ok(())
}

#[test]
fn apply_to_names_the_surfaces_and_an_empty_list_turns_the_marks_off() -> anyhow::Result<()> {
    for (value, sidebar, terminal) in [
        (r#"["sidebar", "terminal"]"#, true, true),
        (r#"["terminal", "sidebar", "sidebar"]"#, true, true),
        (r#"["sidebar"]"#, true, false),
        (r#"["terminal"]"#, false, true),
        ("[]", false, false),
    ] {
        let config = Config::parse(&format!("[worktrees]\napply_to = {value}"))?;
        assert_eq!(
            (config.worktrees.sidebar, config.worktrees.terminal),
            (sidebar, terminal),
            "{value}"
        );
    }
    // The managed file's list is replaced, not merged, by the overrides.
    let config = Config::parse_layers(
        [
            "[worktrees]\napply_to = [\"sidebar\", \"terminal\"]",
            "[worktrees]\napply_to = []",
        ],
        &Daemon::default(),
    )?;
    assert!(!config.worktrees.sidebar && !config.worktrees.terminal);
    assert!(Config::parse("[worktrees]\napply_to = [\"tabs\"]").is_err());
    assert!(Config::parse("[worktrees]\napply_to = \"sidebar\"").is_err());
    Ok(())
}

#[test]
fn a_tint_is_a_hex_colour_and_anything_else_is_named() -> anyhow::Result<()> {
    for (value, color) in [("#2a3350", 0x2a3350), ("#ABC", 0xaabbcc)] {
        let config = Config::parse(&format!("[worktrees]\ntint = \"{value}\""))?;
        assert_eq!(config.worktrees.tint, Some(color), "{value}");
        // Setting a hue leaves the surfaces alone.
        assert!(config.worktrees.sidebar && !config.worktrees.terminal);
    }
    for value in ["2a3350", "#2a335", "blue", ""] {
        match Config::parse(&format!("[worktrees]\ntint = \"{value}\"")) {
            Err(crate::Error::WorktreeIdentity(worktree_identity::Error::InvalidTint(bad))) => {
                assert_eq!(bad, value);
            }
            other => panic!("{value:?}: {other:?}"),
        }
    }
    let Err(error) = Config::parse("[worktrees]\ntint = \"teal\"") else {
        anyhow::bail!("a named colour loaded");
    };
    assert_eq!(
        error.to_string(),
        "worktrees.tint must be a #rgb or #rrggbb colour, not \"teal\""
    );
    Ok(())
}

#[test]
fn unknown_worktree_keys_are_reported_not_fatal() -> anyhow::Result<()> {
    let config = Config::parse("[worktrees]\nstrength = 3\ntint = \"#7aa2f7\"")?;
    assert_eq!(config.unknown_keys, ["worktrees.strength"]);
    assert_eq!(config.worktrees.tint, Some(0x7aa2f7));
    Ok(())
}

/// `herdr-gpui config set` writes what loading accepts and refuses a bad
/// tint before it reaches the file.
#[test]
fn the_command_line_sets_and_checks_the_table() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let local = temp.0.join("config-gpui.local.toml");
    let daemon = temp.0.join("absent.toml");
    fs::write(&local, "")?;
    assert!(Config::edit_key_at(
        &local,
        &daemon,
        "worktrees.apply_to",
        Some(parse_value("[]"))
    )?);
    let config = Config::parse(&fs::read_to_string(&local)?)?;
    assert!(!config.worktrees.sidebar && !config.worktrees.terminal);
    assert!(
        Config::edit_key_at(&local, &daemon, "worktrees.tint", Some(parse_value("teal"))).is_err()
    );
    assert!(!fs::read_to_string(&local)?.contains("teal"));
    Ok(())
}
