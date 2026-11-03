use super::*;
use crate::config::preferences::Preference;

#[test]
fn focus_follows_mouse_defaults_on_and_is_editable_from_settings_and_cli() -> anyhow::Result<()> {
    assert!(Config::default().focus_follows_mouse);
    assert!(Config::parse("")?.focus_follows_mouse);
    assert!(Config::parse(DEFAULT_CONFIG)?.focus_follows_mouse);
    assert!(!Config::parse("focus_follows_mouse = false")?.focus_follows_mouse);
    assert!(Config::parse("focus_follows_mouse = 'true'").is_err());
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config-gpui.toml");
    let local = path.with_extension("local.toml");
    let daemon = temp.0.join("absent.toml");
    let original = "focus_follows_mouse = false # keep\n[terminal]\nsize = 16\n";
    fs::write(&local, original)?;
    Config::save_preference_path(Preference::FocusFollowsMouse(true), &local)?;
    assert_eq!(
        fs::read_to_string(&local)?,
        original.replace("false", "true")
    );
    assert_eq!(
        Config::effective_value_at(&path, "focus_follows_mouse")?,
        Some((toml::Value::Boolean(true), Layer::Local))
    );
    Config::edit_key_at(
        &local,
        &daemon,
        "focus_follows_mouse",
        Some(parse_value("false")),
    )?;
    assert_eq!(fs::read_to_string(&local)?, original);
    assert!(
        Config::edit_key_at(
            &local,
            &daemon,
            "focus_follows_mouse",
            Some(parse_value("1"))
        )
        .is_err()
    );
    assert_eq!(fs::read_to_string(&local)?, original);
    Ok(())
}
