use super::*;
use crate::config::preferences::Preference;

#[test]
fn finished_notifications_default_on_and_a_local_key_mutes_them() -> anyhow::Result<()> {
    use crate::herdr_settings::Settings as Shared;
    assert!(Config::default().notifications.finished);
    assert!(Config::parse(DEFAULT_CONFIG)?.notifications.finished);
    assert!(
        Config::parse("[notifications]\nfinished = true")?
            .notifications
            .finished
    );
    let mut config = Config::parse("[notifications]\nfinished = false")?;
    assert!(!config.notifications.finished);
    assert!(config.unknown_keys.is_empty(), "{:?}", config.unknown_keys);
    // Herdr has no such key, so shared delivery never turns it back on.
    for delivery in ["system", "herdr", "off"] {
        config.apply_shared_notifications(&Shared::parse_text(&format!(
            "[ui.toast]\ndelivery = '{delivery}'"
        ))?);
        assert!(!config.notifications.finished, "{delivery}");
    }
    assert_eq!(
        config.notifications.delivery(),
        NotificationDelivery::Off,
        "the key mutes one kind, it does not choose a delivery"
    );
    for field in ["finished = 1", "finished = \"false\""] {
        assert!(
            matches!(
                Config::parse(&format!("[notifications]\n{field}")),
                Err(crate::Error::Toml(_))
            ),
            "{field}"
        );
    }
    Ok(())
}

#[test]
fn finished_preference_writes_the_local_key() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    for finished in [false, true] {
        Config::save_preference_path(Preference::NotificationFinished(finished), &path)?;
        let table: toml::Table = toml::from_str(&fs::read_to_string(&path)?)?;
        assert_eq!(table["notifications"]["finished"].as_bool(), Some(finished));
    }
    Ok(())
}
