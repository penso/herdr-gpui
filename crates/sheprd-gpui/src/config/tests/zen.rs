use super::*;

#[test]
fn zen_stays_in_the_window_by_default() -> anyhow::Result<()> {
    for config in [
        Config::default(),
        Config::parse("")?,
        Config::parse(DEFAULT_CONFIG)?,
    ] {
        assert_eq!(config.zen, Zen::default());
        assert!(!config.zen.fullscreen);
    }
    Ok(())
}

#[test]
fn zen_parses_and_reports_unknown_keys() -> anyhow::Result<()> {
    let config = Config::parse("[zen]\nfullscreen = true\nfont_scale = 1.2\n")?;
    assert_eq!(
        config.zen,
        Zen {
            fullscreen: true,
            ..Zen::default()
        }
    );
    assert_eq!(config.unknown_keys, ["zen.font_scale"]);
    Ok(())
}

#[test]
fn a_wrongly_typed_zen_key_fails_to_load() {
    assert!(Config::parse("[zen]\nfullscreen = 'yes'\n").is_err());
}

#[test]
fn zen_width_reads_columns_or_a_percentage() -> anyhow::Result<()> {
    for (text, width) in [
        ("120", ZenWidth::Columns(120)),
        ("20", ZenWidth::Columns(20)),
        ("'80%'", ZenWidth::Percent(80)),
        ("' 100 % '", ZenWidth::Percent(100)),
    ] {
        let config = Config::parse(&format!("[zen]\nwidth = {text}\n"))?;
        assert_eq!(config.zen.width, Some(width), "{text}");
    }
    Ok(())
}

#[test]
fn an_unusable_zen_width_fails_with_its_own_error() {
    for text in [
        "0", "19", "-80", "70000", "'5%'", "'101%'", "'80'", "'wide'", "1.5",
    ] {
        let result = Config::parse(&format!("[zen]\nwidth = {text}\n"));
        assert!(
            matches!(
                result,
                Err(crate::Error::Settings(Error::InvalidZenWidth) | crate::Error::Toml(_))
            ),
            "{text}: {result:?}"
        );
    }
    assert!(matches!(
        Config::parse("[zen]\nwidth = 10\n"),
        Err(crate::Error::Settings(Error::InvalidZenWidth))
    ));
}

#[test]
fn zen_font_size_stays_in_the_font_range() -> anyhow::Result<()> {
    assert_eq!(
        Config::parse("[zen]\nfont_size = 24\n")?.zen.font_size,
        Some(24.)
    );
    for size in ["7.5", "49", "nan"] {
        assert!(
            matches!(
                Config::parse(&format!("[zen]\nfont_size = {size}\n")),
                Err(crate::Error::Settings(Error::InvalidZenFontSize))
            ),
            "{size}"
        );
    }
    Ok(())
}
