#![allow(clippy::unwrap_used)]
use super::*;
use std::fs;

mod empty_defaults;
mod lifetime;

#[test]
fn saving_writes_only_edited_keys_and_validates_first() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.local.toml");
    fs::write(
        &path,
        "theme = 'Nord'\n\n[boat]\napi_url = 'https://old.example.com/api' # mine\nallow_plaintext_credentials = true\nmachine_type = 'small'\n",
    )
    .unwrap();
    let fields = BoatFields {
        api_url: "https://boat.dev/api/v1".into(),
        environment: "base".into(),
        ..BoatFields::default()
    };
    Config::save_boat_path(&fields, &path).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("theme = 'Nord'"));
    assert!(text.contains("api_url = \"https://boat.dev/api/v1\" # mine"));
    assert!(text.contains("allow_plaintext_credentials = true"));
    let parsed: toml::Value = toml::from_str(&text).unwrap();
    assert!(parsed["boat"].get("machine_type").is_none());
    assert!(!text.contains("api_key"));
    let broken = BoatFields {
        api_url: "ftp://boat.dev".into(),
        ..BoatFields::default()
    };
    assert!(broken.validate(&BoatConfig::default()).is_err());
}

#[test]
fn loading_reads_the_table_and_refuses_bad_values() {
    let config = Config::parse("[boat]\napi_url = \"https://boat.dev/api/v1\"\n").unwrap();
    assert_eq!(
        config.boat.api_url.as_deref(),
        Some("https://boat.dev/api/v1")
    );
    assert!(Config::parse("[boat]\napi_url = \"http://boat.dev\"\n").is_err());
    assert!(
        Config::parse(
            "[boat]\nmachine_type = \"enormous\"\napi_url = \"https://boat.dev/api/v1\"\n"
        )
        .is_err()
    );
    assert!(Config::parse("[boat]\napi_key = \"secret\"\n").is_err());
}
