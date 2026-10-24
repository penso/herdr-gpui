use super::*;
use crate::apns::config::{apple_id, bundle_id, config_path, dir, key_path};
use p256::pkcs8::{EncodePrivateKey as _, LineEnding};
use std::fs;

fn write_private(path: &std::path::Path, contents: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn pem() -> String {
    SecretKey::from_slice(&[7; 32])
        .unwrap()
        .to_pkcs8_pem(LineEnding::LF)
        .unwrap()
        .to_string()
}

fn config_json(sandbox: bool) -> String {
    format!(r#"{{"team_id":"{TEAM}","key_id":"{KEY_ID}","topic":"{TOPIC}","sandbox":{sandbox}}}"#)
}

#[test]
fn absent_config_turns_the_feature_off() {
    let state = tempfile::tempdir().unwrap();
    assert!(Config::load(state.path()).unwrap().is_none());
    assert!(Sender::load(state.path()).unwrap().is_none());
}

#[test]
fn loads_the_key_and_config_from_the_state_directory() {
    let state = tempfile::tempdir().unwrap();
    write_private(&config_path(state.path()), config_json(true).as_bytes());
    write_private(&key_path(state.path()), pem().as_bytes());
    let config = Config::load(state.path()).unwrap().unwrap();
    assert_eq!(config.team_id, TEAM);
    assert_eq!(config.key_id, KEY_ID);
    assert_eq!(config.host, Host::Sandbox);
    assert_eq!(config.key.verifying_key(), key().verifying_key());
    assert_eq!(
        config.live_activity_topic(),
        "so.pen.herdr-remote.push-type.liveactivity"
    );
    assert_eq!(dir(state.path()), state.path().join("apns"));
}

#[test]
fn production_unless_sandbox_is_set() {
    let state = tempfile::tempdir().unwrap();
    write_private(
        &config_path(state.path()),
        format!(r#"{{"team_id":"{TEAM}","key_id":"{KEY_ID}","topic":"{TOPIC}"}}"#).as_bytes(),
    );
    write_private(&key_path(state.path()), pem().as_bytes());
    let config = Config::load(state.path()).unwrap().unwrap();
    assert_eq!(config.host, Host::Production);
}

#[test]
fn a_config_without_its_key_is_an_error() {
    let state = tempfile::tempdir().unwrap();
    write_private(&config_path(state.path()), config_json(false).as_bytes());
    let error = Config::load(state.path()).unwrap_err();
    assert!(matches!(error, Error::KeyRead { .. }), "{error:?}");
}

#[cfg(unix)]
#[test]
fn a_key_other_users_can_read_is_refused() {
    use std::os::unix::fs::PermissionsExt as _;
    let state = tempfile::tempdir().unwrap();
    write_private(&config_path(state.path()), config_json(false).as_bytes());
    let path = key_path(state.path());
    write_private(&path, pem().as_bytes());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let error = Config::load(state.path()).unwrap_err();
    assert!(matches!(error, Error::KeyNotPrivate { .. }), "{error:?}");
}

#[test]
fn a_key_that_is_not_pkcs8_is_refused() {
    let state = tempfile::tempdir().unwrap();
    write_private(&config_path(state.path()), config_json(false).as_bytes());
    write_private(&key_path(state.path()), b"not a key");
    let error = Config::load(state.path()).unwrap_err();
    assert!(matches!(error, Error::KeyDecode { .. }), "{error:?}");
}

#[test]
fn unknown_fields_and_bad_identifiers_are_refused() {
    let state = tempfile::tempdir().unwrap();
    write_private(&key_path(state.path()), pem().as_bytes());
    write_private(
        &config_path(state.path()),
        br#"{"team_id":"ABCDE12345","key_id":"XYZ9876543","topic":"a.b","host":"evil.example"}"#,
    );
    let error = Config::load(state.path()).unwrap_err();
    assert!(matches!(error, Error::ConfigFormat { .. }), "{error:?}");

    let bad = super::super::config::from_parts("abc", KEY_ID, TOPIC, false, key()).unwrap_err();
    assert!(matches!(bad, Error::ConfigField { field: "team_id" }));
    let bad = super::super::config::from_parts(TEAM, "KEY", TOPIC, false, key()).unwrap_err();
    assert!(matches!(bad, Error::ConfigField { field: "key_id" }));
    let bad = super::super::config::from_parts(TEAM, KEY_ID, "a/b", false, key()).unwrap_err();
    assert!(matches!(bad, Error::ConfigField { field: "topic" }));
}

#[test]
fn identifier_rules() {
    assert!(apple_id("ABCDE12345"));
    assert!(!apple_id("abcde12345"));
    assert!(!apple_id("ABCDE1234"));
    assert!(bundle_id("so.pen.herdr-remote"));
    assert!(!bundle_id("so..pen"));
    assert!(!bundle_id(".so"));
    assert!(!bundle_id("so pen"));
    assert!(!bundle_id(""));
}
