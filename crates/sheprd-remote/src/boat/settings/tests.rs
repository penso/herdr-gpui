#![allow(clippy::unwrap_used)]
use super::*;
use crate::config::BoatConfig;

#[test]
fn resolves_cloud_and_loopback_urls_and_options() {
    let settings = Settings::resolve(
        &BoatConfig {
            api_url: Some("https://boat.dev/api/v1/".into()),
            organization: Some("team_alpha".into()),
            machine_type: Some("large".into()),
            environment: Some("base".into()),
            ..BoatConfig::default()
        },
        |_| None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(settings.base, "https://boat.dev/api/v1");
    assert_eq!(settings.account(), "https://boat.dev/api/v1#team_alpha");
    assert!(
        Settings::resolve(
            &BoatConfig {
                api_url: Some("http://127.0.0.1:1234/api".into()),
                ..BoatConfig::default()
            },
            |_| None
        )
        .is_ok()
    );
    assert!(
        Settings::resolve(
            &BoatConfig {
                api_url: Some("http://boat.dev/api".into()),
                ..BoatConfig::default()
            },
            |_| None
        )
        .is_err()
    );
}

#[test]
fn environment_url_overrides_file_and_bad_options_are_rejected() {
    let config = BoatConfig {
        api_url: Some("https://file.example/api".into()),
        ..BoatConfig::default()
    };
    let settings = Settings::resolve(&config, |name| {
        (name == "HERDR_BOAT_API_URL").then(|| "https://env.example/api".into())
    })
    .unwrap()
    .unwrap();
    assert_eq!(settings.base, "https://env.example/api");
    assert!(
        Settings::resolve(
            &BoatConfig {
                api_url: Some("https://boat.dev/api/v1".into()),
                machine_type: Some("huge".into()),
                ..BoatConfig::default()
            },
            |_| None
        )
        .is_err()
    );
}
