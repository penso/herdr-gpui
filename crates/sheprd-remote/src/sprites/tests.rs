#![allow(clippy::unwrap_used)]
use super::*;
use crate::config::SpritesConfig;

mod relay;

fn settings(url: &str) -> Settings {
    Settings::resolve(
        &SpritesConfig {
            api_url: Some(url.into()),
            ..SpritesConfig::default()
        },
        |_| None,
    )
    .unwrap()
    .unwrap()
}

#[test]
fn the_api_root_must_be_https_or_loopback() {
    assert_eq!(
        settings("https://api.sprites.dev/").base,
        "https://api.sprites.dev"
    );
    assert_eq!(settings("http://127.0.0.1:9/").base, "http://127.0.0.1:9");
    for bad in [
        "http://api.sprites.dev",
        "https://user:pass@api.sprites.dev",
        "https://api.sprites.dev/?x=1",
        "ftp://api.sprites.dev",
    ] {
        let config = SpritesConfig {
            api_url: Some(bad.into()),
            ..SpritesConfig::default()
        };
        assert!(Settings::resolve(&config, |_| None).is_err(), "{bad}");
    }
    assert!(
        Settings::resolve(&SpritesConfig::default(), |_| None)
            .unwrap()
            .is_none(),
        "no table, no account"
    );
}

#[test]
fn the_exec_stream_runs_the_command_with_sh_and_stdin_on() {
    let url = settings("https://api.sprites.dev")
        .exec_url("herdr-calm-otter-1a", "/bin/sh -c 'echo a&b'")
        .unwrap();
    assert_eq!(url.scheme(), "wss");
    assert_eq!(url.path(), "/v1/sprites/herdr-calm-otter-1a/exec");
    let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    assert_eq!(
        pairs,
        [
            ("cmd".into(), "/bin/sh".into()),
            ("cmd".into(), "-c".into()),
            ("cmd".into(), "/bin/sh -c 'echo a&b'".into()),
            ("stdin".into(), "true".into()),
        ]
    );
    let local = settings("http://127.0.0.1:9")
        .exec_url("s", "true")
        .unwrap();
    assert_eq!(local.scheme(), "ws");
}

#[test]
fn sprite_names_stay_plain() {
    use super::settings::sprite_name;
    assert!(sprite_name("herdr-calm-otter-1a"));
    for bad in ["", "-x", "Upper", "a/b", "a b", &"x".repeat(64)] {
        assert!(!sprite_name(bad), "{bad}");
    }
}

#[test]
fn error_replies_keep_one_short_line() {
    assert_eq!(
        http::message(br#"{"error":"sprite\nnot found"}"#).as_deref(),
        Some("sprite not found")
    );
    assert_eq!(
        http::message(br#"{"message":"quota"}"#).as_deref(),
        Some("quota")
    );
    assert_eq!(http::message(b"<html>"), None);
    assert!(matches!(
        http::failure("test", 401, b""),
        Error::Authentication
    ));
    assert!(matches!(
        http::failure("test", 404, br#"{"error":"gone"}"#),
        Error::Status(Status { code: 404, .. })
    ));
}

#[test]
fn the_relay_helper_needs_an_account_a_sprite_and_a_command() {
    use std::ffi::OsString;
    let args = |parts: &[&str]| parts.iter().map(OsString::from).collect::<Vec<_>>();
    assert!(relay::run_helper(&args(&["browser"])).is_none());
    assert_eq!(
        relay::run_helper(&args(&[relay::HELPER, "https://api.sprites.dev", "s"])),
        Some(std::process::ExitCode::from(2))
    );
}
