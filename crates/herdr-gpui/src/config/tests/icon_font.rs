use super::*;

fn without_nerd_fonts() -> Vec<String> {
    ["Menlo", "DejaVu Sans Mono", "DejaVu Sans", "Cascadia Mono"]
        .map(str::to_owned)
        .into()
}

#[test]
fn reports_a_terminal_left_without_an_icon_font() -> anyhow::Result<()> {
    let mut config = Config::parse("")?;
    config.resolve_fonts(without_nerd_fonts);
    assert!(config.icon_font_missing);
    assert_eq!(config.terminal.fallbacks.as_deref(), Some([].as_slice()));
    Ok(())
}

#[test]
fn an_installed_nerd_font_is_not_reported() -> anyhow::Result<()> {
    let mut config = Config::parse("")?;
    config.resolve_fonts(|| {
        let mut installed = without_nerd_fonts();
        installed.push("Symbols Nerd Font Mono".into());
        installed
    });
    assert!(!config.icon_font_missing);
    Ok(())
}

#[test]
fn a_configured_terminal_fallback_is_never_reported() -> anyhow::Result<()> {
    // Even an empty list is a choice the user made, not a detection result.
    for fallback in ["[]", "['Menlo']"] {
        let mut config = Config::parse(&format!("[terminal]\nfallback = {fallback}"))?;
        config.resolve_fonts(without_nerd_fonts);
        assert!(!config.icon_font_missing, "fallback = {fallback}");
    }
    Ok(())
}

#[test]
fn other_faces_without_icons_are_not_reported() -> anyhow::Result<()> {
    // Only the terminal draws prompts.
    let mut config = Config::parse("[terminal]\nfallback = []")?;
    config.resolve_fonts(without_nerd_fonts);
    assert_eq!(config.ui.fallbacks.as_deref(), Some([].as_slice()));
    assert!(!config.icon_font_missing);
    Ok(())
}
