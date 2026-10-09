use super::*;

fn installed() -> Vec<String> {
    ["Menlo", "JetBrains Mono", "Symbols Nerd Font Mono"]
        .map(str::to_owned)
        .into()
}

#[test]
fn configured_families_that_are_not_installed_are_reported() -> anyhow::Result<()> {
    let mut config = Config::parse(
        "[terminal]\nfamily = 'Nope Mono'\nfallback = ['Symbols Nerd Font Mono', 'Gone Icons']\n\
         [sidebar]\nfamily = 'Nope Mono'\n[ui]\nfamily = 'jetbrains mono'",
    )?;
    config.resolve_fonts(installed);
    // Sorted and deduplicated; names match without regard to case.
    assert_eq!(config.missing_fonts, ["Gone Icons", "Nope Mono"]);
    assert_eq!(
        config.diagnostic().as_deref(),
        Some("Configured fonts not installed: \"Gone Icons\", \"Nope Mono\"")
    );
    Ok(())
}

#[test]
fn defaults_and_aliases_are_never_reported() -> anyhow::Result<()> {
    // Defaults are substituted when missing, and dot-prefixed aliases are
    // resolved by GPUI without ever being listed as installed.
    let mut config = Config::parse("[ui]\nfamily = '.SystemUIFont'")?;
    config.resolve_fonts(Vec::<String>::new);
    assert!(config.missing_fonts.is_empty());
    assert_eq!(config.diagnostic(), None);
    Ok(())
}

#[test]
fn a_configured_family_is_checked_even_when_every_cascade_is_set() -> anyhow::Result<()> {
    let mut config = Config::parse(
        "[sidebar]\nfallback = []\n[tabs]\nfallback = []\n\
         [terminal]\nfamily = 'Nope Mono'\nfallback = []\n[ui]\nfallback = []",
    )?;
    config.resolve_fonts(installed);
    assert_eq!(config.missing_fonts, ["Nope Mono"]);
    Ok(())
}

#[test]
fn unknown_keys_and_missing_fonts_share_the_warning_on_separate_lines() -> anyhow::Result<()> {
    let mut config = Config::parse("future_key = 1\n[terminal]\nfamily = 'Nope Mono'")?;
    config.resolve_fonts(installed);
    let diagnostic = config.diagnostic().unwrap_or_default();
    let lines: Vec<&str> = diagnostic.lines().collect();
    assert_eq!(lines.len(), 2, "{diagnostic}");
    assert!(lines[0].contains("future_key"));
    assert!(lines[1].contains("\"Nope Mono\""));
    Ok(())
}
