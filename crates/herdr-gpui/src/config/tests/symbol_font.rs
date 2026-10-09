use super::*;
use crate::config::fonts::REPORTS_MISSING_SYMBOL_FONT;

fn resolved(config: &str, installed: &[&str]) -> anyhow::Result<Config> {
    let mut config = Config::parse(config)?;
    config.resolve_fonts(|| installed.iter().map(|family| (*family).to_owned()));
    Ok(config)
}

#[test]
fn a_machine_without_an_icon_font_is_reported_where_that_means_boxes() -> anyhow::Result<()> {
    assert!(!Config::default().symbol_font_missing);
    let config = resolved("", &["Menlo", "Monaco", "Zapfino"])?;
    // Only macOS is known to draw boxes with an empty cascade.
    assert_eq!(config.symbol_font_missing, REPORTS_MISSING_SYMBOL_FONT);
    assert_eq!(REPORTS_MISSING_SYMBOL_FONT, cfg!(target_os = "macos"));
    // Detection still ran: the face is resolved, to an empty cascade.
    assert_eq!(config.terminal.fallbacks.as_deref(), Some([].as_slice()));
    // A text face the user chose draws no icons either.
    let config = resolved("[terminal]\nfamily = 'JetBrains Mono'", &["JetBrains Mono"])?;
    assert_eq!(config.symbol_font_missing, REPORTS_MISSING_SYMBOL_FONT);
    Ok(())
}

#[test]
fn an_installed_icon_font_is_not_reported() -> anyhow::Result<()> {
    for installed in [
        ["Menlo", "Symbols Nerd Font Mono"],
        ["Menlo", "JetBrainsMono Nerd Font"],
    ] {
        assert!(!resolved("", &installed)?.symbol_font_missing);
    }
    Ok(())
}

#[test]
fn a_terminal_face_that_draws_its_own_icons_is_not_reported() -> anyhow::Result<()> {
    // Detection finds no "Nerd Font" family in any of these, but the terminal
    // face is itself patched, by name.
    for family in [
        "MesloLGS NF",
        "JetBrainsMonoNL NFM",
        "FiraCode NFP",
        "Hack Nerd Font Mono",
        "Meslo LG S for Powerline",
    ] {
        let config = resolved(&format!("[terminal]\nfamily = '{family}'"), &[family])?;
        assert!(!config.symbol_font_missing, "{family}");
    }
    // `NF` counts as a word, not as letters inside one.
    let config = resolved("[terminal]\nfamily = 'Confetti Mono'", &["Confetti Mono"])?;
    assert_eq!(config.symbol_font_missing, REPORTS_MISSING_SYMBOL_FONT);
    // The platform finds a family whatever the case it is written in.
    let config = resolved("[terminal]\nfamily = 'meslolgs nf'", &["MesloLGS NF"])?;
    assert!(!config.symbol_font_missing);
    Ok(())
}

#[test]
fn a_patched_terminal_face_that_is_not_installed_is_reported() -> anyhow::Result<()> {
    // A config copied from another machine names the font without bringing
    // it: nothing draws the icons, so the name must not hide the notice.
    for family in ["MesloLGS NF", "Hack Nerd Font Mono"] {
        let config = resolved(&format!("[terminal]\nfamily = '{family}'"), &["Menlo"])?;
        assert_eq!(
            config.symbol_font_missing, REPORTS_MISSING_SYMBOL_FONT,
            "{family}"
        );
    }
    Ok(())
}

#[test]
fn a_configured_terminal_cascade_is_the_users_choice() -> anyhow::Result<()> {
    // An explicit list, even an empty or uninstalled one, is not a missing
    // font, whatever the other faces are left to detect.
    for terminal in ["fallback = []", "fallback = ['Not Installed']"] {
        let config = resolved(&format!("[terminal]\n{terminal}"), &["Menlo"])?;
        assert!(!config.symbol_font_missing, "{terminal}");
    }
    // The other faces opting out does not hide the terminal's missing font.
    let config = resolved("[ui]\nfallback = []\n[tabs]\nfallback = []", &["Menlo"])?;
    assert_eq!(config.symbol_font_missing, REPORTS_MISSING_SYMBOL_FONT);
    Ok(())
}
