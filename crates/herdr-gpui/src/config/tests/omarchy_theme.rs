use super::*;
use crate::config::omarchy::{NAME, files, installed, load};
use theme::theme_files_in;
use watch::fingerprint_all;

const STATE: &str = ".local/state/omarchy/current/theme";

/// A theme as `omarchy-theme-set` renders it: the Ghostty template's keys,
/// including ones Herdr UI skips, and the tokens it was rendered from.
fn render(directory: &Path, background: &str, accent: &str) -> anyhow::Result<()> {
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join("ghostty.conf"),
        format!(
            "background = {background}\nforeground = #a9b1d6\ncursor-color = #c0caf5\n\
             selection-background = #292e42\npalette = 0={background}\npalette = 5=#ad8ee6\n"
        ),
    )?;
    fs::write(
        directory.join("colors.toml"),
        format!("mode = \"dark\"\naccent = \"{accent}\"\nselection = \"#292e42\"\n"),
    )?;
    Ok(())
}

#[test]
fn an_omarchy_desktop_follows_its_theme_until_the_user_picks_one() -> anyhow::Result<()> {
    let home = TempDirectory::new()?;
    let follow = |overrides: &str| -> anyhow::Result<String> {
        let mut config = Config::parse(overrides)?;
        config.follow_omarchy(overrides, &home.0);
        Ok(config.theme)
    };
    assert_eq!(follow("")?, "Default", "no Omarchy, no change");

    render(&home.0.join(STATE), "#1a1b26", "#7aa2f7")?;
    assert!(installed(&home.0));
    assert_eq!(follow("")?, NAME);
    assert_eq!(follow("layout = \"compact\"\n")?, NAME);
    assert_eq!(follow("theme = \"Nord\"\n")?, "Nord");
    assert_eq!(
        follow("theme = \"Default\"\n")?,
        "Default",
        "an explicit pick stays"
    );
    Ok(())
}

#[test]
fn older_omarchy_releases_keep_the_theme_under_config() -> anyhow::Result<()> {
    let home = TempDirectory::new()?;
    // A directory without the rendered Ghostty theme is not an applied theme.
    fs::create_dir_all(home.0.join(STATE))?;
    assert!(!installed(&home.0));
    let legacy = home.0.join(".config/omarchy/current/theme");
    render(&legacy, "#2e3440", "#81a1c1")?;
    assert!(installed(&home.0));
    assert_eq!(files(&home.0)[0], legacy.join("ghostty.conf"));
    assert_eq!(load(&home.0)?.background, 0x2e3440);
    Ok(())
}

#[test]
fn tokens_color_the_chrome_and_ghostty_colors_the_terminal() -> anyhow::Result<()> {
    let home = TempDirectory::new()?;
    let directory = home.0.join(STATE);
    render(&directory, "#1a1b26", "#7aa2f7")?;
    let theme = load(&home.0)?;
    assert_eq!(theme.background, 0x1a1b26);
    assert_eq!(theme.cursor, 0xc0caf5);
    assert_eq!(theme.palette[5], 0xad8ee6);
    assert_eq!(theme.primary(), 0x7aa2f7, "the accent, not magenta");
    assert_eq!(theme.active, 0x292e42);

    // Tokens Omarchy may not write, or write in a newer shape, fall back.
    fs::write(
        directory.join("colors.toml"),
        "accent = 7\nselection = \"blue\"\n",
    )?;
    let fallback = load(&home.0)?;
    assert_eq!(fallback.primary(), 0xad8ee6);
    assert_eq!(fallback.active, mix(0x1a1b26, 0xa9b1d6, 12));
    fs::write(directory.join("colors.toml"), "not toml [")?;
    assert_eq!(load(&home.0)?.primary(), 0xad8ee6);
    fs::remove_file(directory.join("colors.toml"))?;
    assert_eq!(load(&home.0)?.primary(), 0xad8ee6);

    fs::write(directory.join("ghostty.conf"), "background = nope\n")?;
    assert!(matches!(load(&home.0), Err(crate::Error::Path { .. })));
    Ok(())
}

#[test]
fn switching_the_desktop_theme_changes_the_watched_files() -> anyhow::Result<()> {
    let home = TempDirectory::new()?;
    let current = home.0.join(STATE);
    // Watched before any theme is applied, so applying the first one reloads.
    assert_eq!(files(&home.0)[0], current.join("ghostty.conf"));
    let empty = fingerprint_all(files(&home.0));
    render(&current, "#1a1b26", "#7aa2f7")?;
    let tokyo = fingerprint_all(files(&home.0));
    assert_ne!(empty, tokyo);

    // `omarchy-theme-set` renders beside the current theme, removes it, and
    // renames the new directory into its place.
    let next = home.0.join(".local/state/omarchy/current/next-theme");
    render(&next, "#2e3440", "#81a1c1")?;
    fs::remove_dir_all(&current)?;
    assert_eq!(fingerprint_all(files(&home.0)), empty);
    fs::rename(&next, &current)?;
    assert_ne!(fingerprint_all(files(&home.0)), tokyo);
    assert_eq!(load(&home.0)?.primary(), 0x81a1c1);
    Ok(())
}

#[test]
fn omarchy_is_never_looked_up_as_a_theme_file() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    fs::write(temp.0.join(NAME), "background=120d0a")?;
    let directories = || Ok(vec![temp.0.clone()]);
    assert!(theme_files_in(NAME, directories).is_empty());
    assert!(theme_files_in(&ThemeName::system("Nord", NAME), directories).is_empty());
    Ok(())
}
