#![allow(clippy::unwrap_used)]
use super::*;
use std::fs;

#[test]
fn saving_writes_the_api_root_and_validates_first() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.local.toml");
    fs::write(&path, "[sprites]\nallow_plaintext_credentials = true\n").unwrap();
    let fields = SpritesFields {
        api_url: "https://api.sprites.dev".into(),
    };
    Config::save_sprites_path(&fields, &path).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("api_url = \"https://api.sprites.dev\""));
    assert!(text.contains("allow_plaintext_credentials = true"));
    assert!(!text.contains("token"), "the token never goes to the file");
    let broken = SpritesFields {
        api_url: "http://sprites.example.com".into(),
    };
    assert!(broken.validate(&SpritesConfig::default()).is_err());
}

#[test]
fn loading_reads_the_table_and_refuses_bad_values() {
    let config = Config::parse("[sprites]\napi_url = \"https://api.sprites.dev\"\n").unwrap();
    assert_eq!(
        config.sprites.api_url.as_deref(),
        Some("https://api.sprites.dev")
    );
    assert!(Config::parse("[sprites]\napi_url = \"ftp://sprites.dev\"\n").is_err());
    assert!(Config::parse("[sprites]\ntoken = \"secret\"\n").is_err());
}
