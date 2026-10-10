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

#[test]
fn a_terminal_face_patched_with_the_icons_is_not_reported() -> anyhow::Result<()> {
    // Detection finds no "Nerd Font" family in any of these, but the terminal
    // face draws the icons itself.
    for family in ["MesloLGS NF", "JetBrainsMonoNL NFM", "FiraCode NFP"] {
        let mut config = Config::parse(&format!("[terminal]\nfamily = '{family}'"))?;
        config.resolve_fonts(|| {
            let mut installed = without_nerd_fonts();
            installed.push(family.to_owned());
            installed
        });
        assert!(!config.icon_font_missing, "{family}");
        assert!(config.missing_fonts.is_empty(), "{family}");
    }
    // The platform finds a family whatever the case it is written in.
    let mut config = Config::parse("[terminal]\nfamily = 'meslolgs nf'")?;
    config.resolve_fonts(|| {
        let mut installed = without_nerd_fonts();
        installed.push("MesloLGS NF".into());
        installed
    });
    assert!(!config.icon_font_missing);
    Ok(())
}

#[test]
fn a_patched_terminal_face_that_is_not_installed_is_still_reported() -> anyhow::Result<()> {
    // A config copied from another machine names the font without bringing
    // it: nothing draws the icons, so its name must not hide the notice.
    let mut config = Config::parse("[terminal]\nfamily = 'MesloLGS NF'")?;
    config.resolve_fonts(without_nerd_fonts);
    assert!(config.icon_font_missing);
    assert_eq!(config.missing_fonts, ["MesloLGS NF"]);
    Ok(())
}

#[test]
fn a_terminal_face_without_the_icons_is_still_reported() -> anyhow::Result<()> {
    // `NF` counts as a word, not as letters inside one. A Powerline face
    // draws separators and the branch mark, but no folder or logo.
    for family in [
        "JetBrains Mono",
        "Confetti Mono",
        "Meslo LG S for Powerline",
    ] {
        let mut config = Config::parse(&format!("[terminal]\nfamily = '{family}'"))?;
        config.resolve_fonts(|| {
            let mut installed = without_nerd_fonts();
            installed.push(family.to_owned());
            installed
        });
        assert!(config.icon_font_missing, "{family}");
    }
    Ok(())
}
