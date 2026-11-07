#![allow(clippy::unwrap_used)]
use super::*;
use std::fs;

#[test]
fn saving_writes_the_server_and_sandbox_size_and_validates_first() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.local.toml");
    fs::write(&path, "[opensandbox]\nallow_plaintext_credentials = true\n").unwrap();
    let fields = OpenSandboxFields {
        api_url: "http://127.0.0.1:8080".into(),
        memory: "8Gi".into(),
        ..OpenSandboxFields::default()
    };
    Config::save_opensandbox_path(&fields, &path).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("api_url = \"http://127.0.0.1:8080\""));
    assert!(text.contains("memory = \"8Gi\""));
    assert!(text.contains("allow_plaintext_credentials = true"));
    for broken in [
        OpenSandboxFields {
            api_url: "http://sandbox.example.com".into(),
            ..OpenSandboxFields::default()
        },
        OpenSandboxFields {
            api_url: "http://127.0.0.1:8080".into(),
            cpu: "lots".into(),
            ..OpenSandboxFields::default()
        },
    ] {
        assert!(broken.validate(&OpenSandboxConfig::default()).is_err());
    }
}

#[test]
fn loading_reads_the_table_and_refuses_bad_values() {
    let config =
        Config::parse("[opensandbox]\napi_url = \"https://sandbox.example.com\"\n").unwrap();
    assert_eq!(
        config.opensandbox.api_url.as_deref(),
        Some("https://sandbox.example.com")
    );
    assert!(Config::parse("[opensandbox]\napi_url = \"http://10.0.0.5:8080\"\n").is_err());
    assert!(Config::parse("[opensandbox]\napi_key = \"secret\"\n").is_err());
}
