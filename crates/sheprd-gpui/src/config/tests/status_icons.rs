use super::*;

/// An unset key draws Dots, whatever Herdr's own `ui.status_indicators`
/// says; every set reads back from its own spelling.
#[test]
fn status_icons_default_to_dots_and_read_every_set() -> anyhow::Result<()> {
    assert_eq!(Config::default().status_icons, StatusIcons::Dots);
    assert_eq!(Config::parse("")?.status_icons, StatusIcons::Dots);
    assert_eq!(
        Config::parse(DEFAULT_CONFIG)?.status_icons,
        StatusIcons::Dots
    );
    assert!(!Config::parse("")?.edge_rail);
    for &icons in StatusIcons::ALL {
        let text = format!("status_icons = '{}'", icons.as_str());
        let config = Config::parse(&text)?;
        assert_eq!(config.status_icons, icons);
        assert!(!config.edge_rail, "{text}");
    }
    for invalid in ["symbols", "superset", "Solid", ""] {
        assert!(Config::parse(&format!("status_icons = '{invalid}'")).is_err());
    }
    Ok(())
}

/// Retired spellings keep what they drew: `rail` was outline icons with the
/// edge rail, unless `edge_rail` says otherwise; `wave-check` reads as the
/// wave set and `dense-braille` as the braille set.
#[test]
fn retired_status_icons_keep_what_they_drew() -> anyhow::Result<()> {
    let config = Config::parse("status_icons = 'rail'")?;
    assert_eq!(
        (config.status_icons, config.edge_rail),
        (StatusIcons::Outline, true)
    );
    let config = Config::parse("status_icons = 'rail'\nedge_rail = false")?;
    assert_eq!(
        (config.status_icons, config.edge_rail),
        (StatusIcons::Outline, false)
    );
    let config = Config::parse("status_icons = 'wave-check'")?;
    assert_eq!(config.status_icons, StatusIcons::Wave);
    let config = Config::parse("status_icons = 'dense-braille'")?;
    assert_eq!(config.status_icons, StatusIcons::Braille);
    let config = Config::parse("status_icons = 'braille'\nedge_rail = true")?;
    assert_eq!(
        (config.status_icons, config.edge_rail),
        (StatusIcons::Braille, true)
    );
    Ok(())
}

#[test]
fn status_icon_preferences_write_their_keys_in_place() -> anyhow::Result<()> {
    use crate::config::preferences::Preference;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    fs::write(&path, "# keep\nfuture = 1\nstatus_icons = 'rail' # mine\n")?;
    Config::save_preference_path(Preference::EdgeRail(true), &path)?;
    Config::save_preference_path(Preference::StatusIcons(StatusIcons::Orca), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("# keep\nfuture = 1"), "{text}");
    assert!(text.contains("status_icons = \"orca\" # mine"), "{text}");
    let config = Config::parse(&text)?;
    assert_eq!(
        (config.status_icons, config.edge_rail),
        (StatusIcons::Orca, true)
    );
    Config::save_preference_path(Preference::EdgeRail(false), &path)?;
    assert!(!Config::parse(&fs::read_to_string(&path)?)?.edge_rail);
    Ok(())
}
