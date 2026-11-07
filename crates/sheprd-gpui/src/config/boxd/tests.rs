#![allow(clippy::unwrap_used)]
use super::*;
use std::fs;

#[test]
fn saving_writes_the_api_origin_keeps_labels_and_validates_first() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.local.toml");
    fs::write(
        &path,
        "[boxd]\nallow_plaintext_credentials = true\nlabels = { team = \"web\" }\n",
    )
    .unwrap();
    let fields = BoxdFields {
        api_url: "https://boxd.sh:9443".into(),
    };
    Config::save_boxd_path(&fields, &path).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("api_url = \"https://boxd.sh:9443\""));
    assert!(text.contains("allow_plaintext_credentials = true"));
    assert!(text.contains("team = \"web\""));
    assert!(!text.contains("key"), "the API key never goes to the file");
    let broken = BoxdFields {
        api_url: "https://boxd.sh:9443/api".into(),
    };
    assert!(broken.validate(&BoxdConfig::default()).is_err());
}

#[test]
fn loading_reads_the_table_and_refuses_bad_values() {
    let config = Config::parse(
        "[boxd]\napi_url = \"https://nyc.boxd.sh:9443\"\nlabels = { team = \"web\" }\n",
    )
    .unwrap();
    assert_eq!(
        config.boxd.api_url.as_deref(),
        Some("https://nyc.boxd.sh:9443")
    );
    assert_eq!(config.boxd.labels["team"], "web");
    assert!(Config::parse("[boxd]\napi_url = \"http://boxd.sh:9443\"\n").is_err());
    assert!(
        Config::parse(
            "[boxd]\napi_url = \"https://boxd.sh:9443\"\nlabels = { \"boxd/x\" = \"y\" }\n"
        )
        .is_err()
    );
    assert!(Config::parse("[boxd]\napi_key = \"secret\"\n").is_err());
}
