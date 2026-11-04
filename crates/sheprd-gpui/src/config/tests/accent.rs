use super::*;

#[test]
fn the_accent_follows_the_theme_unless_the_system_is_asked_for() -> anyhow::Result<()> {
    assert_eq!(Config::default().accent, Accent::Theme);
    assert_eq!(Config::parse("accent = 'theme'\n")?.accent, Accent::Theme);
    assert_eq!(Config::parse("accent = 'system'\n")?.accent, Accent::System);
    assert!(Config::parse("accent = 'blue'\n").is_err());
    assert_eq!(Accent::Theme.color(), None);
    if !cfg!(windows) {
        assert_eq!(Accent::System.color(), None);
    }
    Ok(())
}

#[test]
fn a_system_accent_replaces_the_primary_color_but_no_terminal_color() {
    let theme = Theme::default();
    let accented = theme.clone().with_accent(Some(0x0078d4));
    assert_eq!(theme.primary(), theme.palette[5]);
    assert_eq!(accented.primary(), 0x0078d4);
    assert_eq!(accented.palette, theme.palette);
    assert_eq!(theme.clone().with_accent(None), theme);
}

#[test]
fn the_system_accent_wins_over_a_themes_own() {
    let theme = Theme {
        accent: Some(0x7aa2f7),
        ..Theme::default()
    };
    assert_eq!(theme.primary(), 0x7aa2f7);
    assert_eq!(
        theme.clone().with_accent(Some(0x0078d4)).primary(),
        0x0078d4
    );
    assert_eq!(theme.clone().with_accent(None).primary(), 0x7aa2f7);
}

#[test]
fn styling_a_theme_twice_matches_styling_it_once() {
    let config = Config {
        contrast: Contrast::High,
        ..Config::default()
    };
    let once = Theme::default().styled_for(&config);
    assert_eq!(once.clone().styled_for(&config), once);
    assert_eq!(once.contrast, Contrast::High);
}
