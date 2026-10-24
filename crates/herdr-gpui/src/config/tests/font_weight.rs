use super::*;
use crate::config::{bold_font_weight, font_weight::MissingWeight};
use gpui::FontWeight;

#[test]
fn every_face_draws_regular_text_until_a_weight_is_configured() -> anyhow::Result<()> {
    for config in [
        Config::default(),
        Config::parse("")?,
        Config::parse(DEFAULT_CONFIG)?,
    ] {
        for font in [&config.sidebar, &config.tabs, &config.terminal, &config.ui] {
            assert_eq!(font.weight, FontWeight::NORMAL);
            assert_eq!(font.font().weight, FontWeight::NORMAL);
        }
    }
    Ok(())
}

#[test]
fn terminal_weight_takes_css_names_and_numbers() -> anyhow::Result<()> {
    for (value, weight) in [
        ("'medium'", FontWeight::MEDIUM),
        ("'Medium'", FontWeight::MEDIUM),
        ("'semi-bold'", FontWeight::SEMIBOLD),
        ("'Extra Light'", FontWeight::EXTRA_LIGHT),
        ("'regular'", FontWeight::NORMAL),
        ("'heavy'", FontWeight::BLACK),
        ("100", FontWeight::THIN),
        ("450", FontWeight(450.0)),
        ("900", FontWeight::BLACK),
    ] {
        let config = Config::parse(&format!("[terminal]\nweight = {value}"))?;
        assert_eq!(config.terminal.weight, weight, "weight = {value}");
        assert_eq!(config.terminal.font().weight, weight, "weight = {value}");
        assert!(config.unknown_keys.is_empty(), "{:?}", config.unknown_keys);
    }
    Ok(())
}

#[test]
fn local_weight_merges_over_managed_defaults() -> anyhow::Result<()> {
    let config = Config::parse_layers(
        [DEFAULT_CONFIG, "[terminal]\nweight = 'medium'"],
        &Daemon::default(),
    )?;
    assert_eq!(config.terminal.size, 14.0);
    assert_eq!(config.terminal.weight, FontWeight::MEDIUM);
    Ok(())
}

#[test]
fn rejects_unknown_names_and_out_of_range_weights() {
    for value in [
        "'mediumish'",
        "''",
        "99",
        "901",
        "0",
        "-400",
        "500.0",
        "true",
        "['medium']",
    ] {
        assert!(
            matches!(
                Config::parse(&format!("[terminal]\nweight = {value}")),
                Err(crate::Error::Settings(Error::InvalidFontWeight("terminal")))
            ),
            "accepted weight = {value}"
        );
    }
}

#[test]
fn weight_outside_terminal_is_reported_and_ignored() -> anyhow::Result<()> {
    // Invalid too: an ignored key is never validated.
    let config =
        Config::parse("[sidebar]\nweight = 'medium'\n[tabs]\nweight = 500\n[ui]\nweight = 'nope'")?;
    assert_eq!(
        config.unknown_keys,
        ["sidebar.weight", "tabs.weight", "ui.weight"]
    );
    for font in [&config.sidebar, &config.tabs, &config.ui] {
        assert_eq!(font.weight, FontWeight::NORMAL);
    }
    Ok(())
}

#[test]
fn bold_cells_stay_heavier_than_the_configured_weight() {
    for (weight, bold) in [
        (FontWeight::THIN, FontWeight::BOLD),
        (FontWeight::LIGHT, FontWeight::BOLD),
        (FontWeight::NORMAL, FontWeight::BOLD),
        (FontWeight::MEDIUM, FontWeight::EXTRA_BOLD),
        (FontWeight::SEMIBOLD, FontWeight::BLACK),
        (FontWeight::BOLD, FontWeight::BLACK),
        (FontWeight::BLACK, FontWeight::BLACK),
    ] {
        assert_eq!(bold_font_weight(weight), bold, "base {weight:?}");
    }
}

fn installed() -> Vec<String> {
    ["Menlo", "JetBrains Mono"].map(str::to_owned).into()
}

#[test]
fn a_weight_the_family_lacks_is_reported() -> anyhow::Result<()> {
    let mut config = Config::parse("[terminal]\nfamily = 'Menlo'\nweight = 'medium'")?;
    config.resolve_fonts_with(installed, |_| false);
    config.check_font_weight(|family, weight| {
        assert_eq!((family, weight), ("Menlo", FontWeight::MEDIUM));
        false
    });
    assert_eq!(
        config.missing_weight,
        Some(MissingWeight {
            family: "Menlo".into(),
            weight: FontWeight::MEDIUM,
        })
    );
    assert_eq!(
        config.diagnostic().as_deref(),
        Some("\"Menlo\" has no medium (500) face; using its nearest weight")
    );

    // A number without a CSS name prints alone.
    let mut config = Config::parse("[terminal]\nfamily = 'Menlo'\nweight = 450")?;
    config.check_font_weight(|_, _| false);
    assert_eq!(
        config.diagnostic().as_deref(),
        Some("\"Menlo\" has no 450 face; using its nearest weight")
    );
    Ok(())
}

#[test]
fn installed_weights_regular_text_and_missing_families_are_not_reported() -> anyhow::Result<()> {
    let mut config = Config::parse("[terminal]\nfamily = 'JetBrains Mono'\nweight = 'medium'")?;
    config.resolve_fonts_with(installed, |_| false);
    config.check_font_weight(|_, _| true);
    assert_eq!(config.missing_weight, None);
    assert_eq!(config.diagnostic(), None);

    // Regular text needs no check: every family draws something there.
    let mut config = Config::parse("[terminal]\nfamily = 'JetBrains Mono'")?;
    config.check_font_weight(|_, _| panic!("regular weight was checked"));
    assert_eq!(config.missing_weight, None);

    // A family that is not installed is reported once, as missing.
    let mut config = Config::parse("[terminal]\nfamily = 'Nope Mono'\nweight = 'medium'")?;
    config.resolve_fonts_with(installed, |_| false);
    config.check_font_weight(|_, _| panic!("a missing family was checked"));
    assert_eq!(config.missing_weight, None);
    assert_eq!(
        config.diagnostic().as_deref(),
        Some("Configured fonts not installed: \"Nope Mono\"")
    );
    Ok(())
}

/// Menlo ships regular and bold faces only, and every macOS release has both.
#[cfg(target_os = "macos")]
#[test]
fn the_platform_lists_a_family_by_weight() {
    use crate::config::font_weight::platform_has_weight;
    assert!(platform_has_weight("Menlo", FontWeight::NORMAL));
    assert!(platform_has_weight("Menlo", FontWeight::BOLD));
    assert!(!platform_has_weight("Menlo", FontWeight::MEDIUM));
    assert!(!platform_has_weight("Menlo", FontWeight::THIN));
    // Helvetica Neue ships a medium face with every macOS release.
    assert!(platform_has_weight("Helvetica Neue", FontWeight::MEDIUM));
}
