use super::*;
use crate::config::preferences::Preference;

#[test]
fn show_menu_bar_defaults_on_and_saves_from_settings() -> anyhow::Result<()> {
    assert!(Config::default().show_menu_bar);
    assert!(Config::parse("")?.show_menu_bar);
    assert!(Config::parse(DEFAULT_CONFIG)?.show_menu_bar);
    assert!(!Config::parse("show_menu_bar = false")?.show_menu_bar);
    assert!(Config::parse("show_menu_bar = 'false'").is_err());

    let temp = TempDirectory::new()?;
    let local = temp.0.join("config-gpui.local.toml");
    fs::write(&local, "show_menu_bar = true # keep\n")?;
    Config::save_preference_path(Preference::ShowMenuBar(false), &local)?;
    assert_eq!(
        fs::read_to_string(&local)?,
        "show_menu_bar = false # keep\n"
    );
    Ok(())
}
