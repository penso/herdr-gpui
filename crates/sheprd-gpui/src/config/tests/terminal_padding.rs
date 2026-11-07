use super::*;
use crate::config::preferences::Preference;

#[allow(clippy::expect_used)]
fn sides(start: f32, end: f32) -> Sides {
    Sides::new(start, end).expect("padding in band")
}

fn padding(text: &str) -> Result<Padding> {
    Ok(Config::parse(text)?.terminal_padding)
}

#[test]
fn terminal_padding_defaults_to_none_on_every_side() -> anyhow::Result<()> {
    for config in [
        Config::default(),
        Config::parse("")?,
        Config::parse(DEFAULT_CONFIG)?,
        Config::parse("[terminal]")?,
        Config::parse("[terminal]\nsize = 18")?,
    ] {
        assert_eq!(config.terminal_padding.x, sides(0., 0.));
        assert_eq!(config.terminal_padding.y, sides(0., 0.));
        assert_eq!(config.diagnostic(), None);
    }
    Ok(())
}

#[test]
fn one_number_pads_both_sides_and_a_pair_pads_each() -> anyhow::Result<()> {
    for (text, x, y) in [
        ("padding_x = 8", sides(8., 8.), sides(0., 0.)),
        ("padding_y = 8", sides(0., 0.), sides(8., 8.)),
        ("padding_x = [2, 4]", sides(2., 4.), sides(0., 0.)),
        ("padding_y = [4, 8]", sides(0., 0.), sides(4., 8.)),
        (
            "padding_x = 7.5\npadding_y = [0, 64]",
            sides(7.5, 7.5),
            sides(0., 64.),
        ),
        (
            "padding_x = [64, 0.5]\npadding_y = 0",
            sides(64., 0.5),
            sides(0., 0.),
        ),
    ] {
        let config = Config::parse(&format!("[terminal]\n{text}"))?;
        assert_eq!(config.terminal_padding.x, x, "{text}");
        assert_eq!(config.terminal_padding.y, y, "{text}");
        // Padding alone leaves the font and the unknown-key report alone.
        assert_eq!(config.terminal, Config::default().terminal);
        assert!(config.unknown_keys.is_empty(), "{text}");
        assert_eq!(config.diagnostic(), None);
    }
    Ok(())
}

#[test]
fn padding_outside_its_band_or_shape_is_refused_by_key() {
    for value in [
        "-1",
        "64.1",
        "65",
        "nan",
        "inf",
        "'8'",
        "true",
        "[]",
        "[1]",
        "[1, 2, 3]",
        "[1, '2']",
        "[1, 65]",
        "[-0.5, 1]",
        "[[1, 2]]",
        "{ left = 1 }",
    ] {
        for key in ["padding_x", "padding_y"] {
            let text = format!("[terminal]\n{key} = {value}");
            assert!(
                matches!(
                    padding(&text),
                    Err(crate::Error::Settings(Error::InvalidTerminalPadding(named))) if named == key
                ),
                "{text}"
            );
        }
    }
}

#[test]
fn only_the_terminal_table_takes_padding() -> anyhow::Result<()> {
    // Even a value the terminal would refuse is only reported elsewhere.
    let config = Config::parse(
        "[ui]\npadding_x = 4\n[tabs]\npadding_y = 'wide'\n[sidebar]\npadding_x = [1, 2]",
    )?;
    assert_eq!(config.terminal_padding, Padding::default());
    assert_eq!(
        config.unknown_keys,
        ["sidebar.padding_x", "tabs.padding_y", "ui.padding_x"]
    );
    Ok(())
}

#[test]
fn the_deprecated_sidebar_gap_stands_in_for_the_left_side_only() -> anyhow::Result<()> {
    let config = Config::parse("[layout]\nmode = 'compact'\nsidebar_gap = 8")?;
    assert_eq!(config.terminal_padding.x, sides(8., 0.));
    assert_eq!(config.terminal_padding.y, sides(0., 0.));
    // It still loads, so it is not an unknown key, but the user is told.
    assert!(config.unknown_keys.is_empty());
    let diagnostic = config.diagnostic().context("deprecation warning")?;
    assert!(diagnostic.contains("layout.sidebar_gap"), "{diagnostic}");
    assert!(diagnostic.contains("terminal.padding_x"), "{diagnostic}");

    // Vertical padding says nothing about the horizontal sides.
    let config = Config::parse("[layout]\nsidebar_gap = 8\n[terminal]\npadding_y = 3")?;
    assert_eq!(config.terminal_padding.x, sides(8., 0.));
    assert_eq!(config.terminal_padding.y, sides(3., 3.));
    Ok(())
}

#[test]
fn padding_x_wins_over_the_deprecated_sidebar_gap() -> anyhow::Result<()> {
    for (text, x) in [
        ("padding_x = 2", sides(2., 2.)),
        ("padding_x = 0", sides(0., 0.)),
        ("padding_x = [1, 3]", sides(1., 3.)),
    ] {
        let config = Config::parse(&format!("[layout]\nsidebar_gap = 8\n[terminal]\n{text}"))?;
        assert_eq!(config.terminal_padding.x, x, "{text}");
        // The stale key is still worth removing.
        assert!(config.diagnostic().is_some());
    }
    // A managed file's padding does not hide a personal file's old gap from
    // the report, and the personal padding wins over both.
    let merged = Config::parse_layers(
        [
            "[terminal]\npadding_x = 4\npadding_y = 4",
            "[layout]\nsidebar_gap = 8\n[terminal]\npadding_y = [1, 2]",
        ],
        &Daemon::default(),
    )?;
    assert_eq!(merged.terminal_padding.x, sides(4., 4.));
    assert_eq!(merged.terminal_padding.y, sides(1., 2.));
    Ok(())
}

#[test]
fn an_unused_deprecated_gap_is_still_held_to_its_band() {
    for gap in ["-1", "64.1", "nan"] {
        for terminal in ["", "[terminal]\npadding_x = 2"] {
            let text = format!("[layout]\nsidebar_gap = {gap}\n{terminal}");
            assert!(
                matches!(
                    padding(&text),
                    Err(crate::Error::Settings(Error::InvalidSidebarGap))
                ),
                "{text}"
            );
        }
    }
}

#[test]
fn saving_horizontal_padding_replaces_the_deprecated_gap() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config-gpui.local.toml");
    for original in [
        "# mine\ntheme = 'Nord'\n\n[layout] # sidebar\nmode = 'compact'\nsidebar_gap = 4 # old\n\n[terminal]\nsize = 18 # big\n",
        "layout = { mode = 'compact', sidebar_gap = 4 }\ntheme = 'Nord'\n",
    ] {
        fs::write(&path, original)?;
        // The other axis leaves the gap, which still pads the left side.
        Config::save_preference_path(Preference::TerminalPadding(Axis::Y, sides(6., 6.)), &path)?;
        let config = Config::parse(&fs::read_to_string(&path)?)?;
        assert_eq!(config.terminal_padding.x, sides(4., 0.));
        assert_eq!(config.terminal_padding.y, sides(6., 6.));
        assert!(config.diagnostic().is_some());

        Config::save_preference_path(Preference::TerminalPadding(Axis::X, sides(12., 12.)), &path)?;
        let text = fs::read_to_string(&path)?;
        assert!(!text.contains("sidebar_gap"), "{text}");
        let config = Config::parse(&text)?;
        assert_eq!(config.terminal_padding.x, sides(12., 12.));
        assert_eq!(config.terminal_padding.y, sides(6., 6.));
        // Nothing deprecated is left to warn about, and nothing else moved.
        assert_eq!(config.diagnostic(), None);
        assert_eq!(config.layout.mode, LayoutMode::from(Density::Compact));
        assert_eq!(config.theme, "Nord");
    }
    Ok(())
}

#[test]
fn saving_padding_keeps_comments_and_a_named_layout() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config-gpui.local.toml");
    fs::write(
        &path,
        "# mine\nlayout = 'orca' # mode\n\n[terminal] # cells\nsize = 18 # big\npadding_x = [2, 4] # sides\n",
    )?;
    Config::save_preference_path(Preference::TerminalPadding(Axis::X, sides(9., 9.)), &path)?;
    Config::save_preference_path(Preference::TerminalPadding(Axis::Y, sides(2.5, 2.5)), &path)?;
    let text = fs::read_to_string(&path)?;
    for kept in [
        "# mine",
        "layout = 'orca' # mode",
        "# cells",
        "size = 18 # big",
        "# sides",
    ] {
        assert!(text.contains(kept), "{kept}: {text}");
    }
    // Whole pixels are written as a person would write them.
    assert!(text.contains("padding_x = 9 "), "{text}");
    let config = Config::parse(&text)?;
    assert_eq!(config.terminal_padding.x, sides(9., 9.));
    assert_eq!(config.terminal_padding.y, sides(2.5, 2.5));
    assert_eq!(config.layout.mode, LayoutMode::Orca);
    assert_eq!(config.terminal.size, 18.);

    // Sides that differ are written as a pair, in place of the number.
    Config::save_preference_path(Preference::TerminalPadding(Axis::X, sides(9., 10.)), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("padding_x = [9, 10] # sides"), "{text}");
    assert_eq!(Config::parse(&text)?.terminal_padding.x, sides(9., 10.));
    // And agreeing sides collapse the pair back to one number.
    Config::save_preference_path(Preference::TerminalPadding(Axis::X, sides(10., 10.)), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("padding_x = 10 # sides"), "{text}");

    // A file with no terminal table gains one.
    fs::write(&path, "theme = 'Nord'\n")?;
    Config::save_preference_path(Preference::TerminalPadding(Axis::Y, sides(64., 64.)), &path)?;
    let config = Config::parse(&fs::read_to_string(&path)?)?;
    assert_eq!(config.terminal_padding.y, sides(64., 64.));
    assert_eq!(config.theme, "Nord");
    Ok(())
}
