//! The GUI config diagnostic is built from messages around names from the
//! user's file, which lose the characters untrusted text does.
use super::*;

#[test]
fn unknown_keys_lose_breaks_and_direction_overrides() -> anyhow::Result<()> {
    let config = Config::parse("\"bad\\nkey\\u202e\" = 1")?;
    let shown = plain(config.diagnostic()).context("a diagnostic")?;
    assert_eq!(
        shown,
        "config-gpui.local.toml: ignoring unknown keys badkey"
    );
    Ok(())
}

#[test]
fn a_short_list_names_no_remainder() -> anyhow::Result<()> {
    let config = Config::parse("a = 1\nb = 2")?;
    assert_eq!(
        plain(config.diagnostic()).as_deref(),
        Some("config-gpui.local.toml: ignoring unknown keys a, b")
    );
    Ok(())
}
