use super::*;

#[test]
fn lifetime_round_trips_and_rejects_unsupported_deadlines() {
    for (text, expected) in [
        ("never", None),
        ("1", Some(1)),
        ("7200", Some(7200)),
        ("2592000", Some(2592000)),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let fields = BoatFields {
            api_url: crate::boat::DEFAULT_API_URL.into(),
            auto_stop: text.into(),
            ..BoatFields::default()
        };
        fields.validate(&BoatConfig::default()).unwrap();
        Config::save_boat_path(&fields, &path).unwrap();
        let config = Config::parse(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            config
                .boat
                .settings_with(|_| None)
                .unwrap()
                .unwrap()
                .ttl_seconds,
            expected
        );
        assert_eq!(BoatFields::from_config(&config.boat).auto_stop, text);
    }
    for value in ["0", "-1", "2592001", "1h", "sometimes"] {
        let config = format!(
            "[boat]\napi_url = {:?}\nauto_stop = {value:?}\n",
            crate::boat::DEFAULT_API_URL
        );
        assert!(Config::parse(&config).is_err(), "{value}");
    }
}

#[test]
fn cli_setting_edits_validate_lifetimes_and_preserve_the_file_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let local = dir.path().join("config.local.toml");
    let daemon = dir.path().join("herdr.toml");
    fs::write(
        &local,
        format!(
            "[boat]\napi_url = {:?}\nauto_stop = 'never' # lifetime\n",
            crate::boat::DEFAULT_API_URL
        ),
    )
    .unwrap();
    Config::edit_key_at(
        &local,
        &daemon,
        "boat.auto_stop",
        Some(crate::config::parse_value("\"7200\"")),
    )
    .unwrap();
    let saved = fs::read_to_string(&local).unwrap();
    assert!(saved.contains("# lifetime"));
    assert_eq!(
        Config::parse(&saved)
            .unwrap()
            .boat
            .settings_with(|_| None)
            .unwrap()
            .unwrap()
            .ttl_seconds,
        Some(7200)
    );
    assert!(
        Config::edit_key_at(
            &local,
            &daemon,
            "boat.auto_stop",
            Some(crate::config::parse_value("\"0\""))
        )
        .is_err()
    );
    assert_eq!(fs::read_to_string(&local).unwrap(), saved);
    Config::edit_key_at(&local, &daemon, "boat.auto_stop", None).unwrap();
    assert_eq!(
        Config::parse(&fs::read_to_string(&local).unwrap())
            .unwrap()
            .boat
            .settings_with(|_| None)
            .unwrap()
            .unwrap()
            .ttl_seconds,
        None
    );
}
