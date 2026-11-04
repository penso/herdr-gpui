#![allow(clippy::unwrap_used)]
use super::*;
use std::fs;

#[test]
fn saving_writes_only_edited_keys_and_validates_first() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.local.toml");
    fs::write(
        &path,
        "theme = 'Nord'\n\n[container]\nimage = 'debian:12' # mine\npids_limit = 512\nmemory = '4g'\n",
    )
    .unwrap();
    let fields = ContainerFields {
        image: "ubuntu:24.04".into(),
        cpus: "2".into(),
        ..ContainerFields::default()
    };
    Config::save_container_path(&fields, &path).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("theme = 'Nord'"));
    assert!(text.contains("image = \"ubuntu:24.04\" # mine"));
    assert!(
        text.contains("pids_limit = 512"),
        "keys Settings does not edit stay"
    );
    let parsed: toml::Value = toml::from_str(&text).unwrap();
    assert!(
        parsed["container"].get("memory").is_none(),
        "an emptied key is removed"
    );
    assert_eq!(parsed["container"]["cpus"].as_str(), Some("2"));
    let broken = ContainerFields {
        memory: "lots".into(),
        ..ContainerFields::default()
    };
    assert!(broken.validate(&ContainerConfig::default()).is_err());
}

#[test]
fn loading_reads_the_table_and_refuses_bad_values() {
    let config = Config::parse("[container]\nengine = \"podman\"\ncpus = 2\n").unwrap();
    assert_eq!(config.container.engine.as_deref(), Some("podman"));
    assert_eq!(config.container.cpus.as_deref(), Some("2"));
    assert!(Config::parse("[container]\nengine = \"lxc\"\n").is_err());
    assert!(Config::parse("[container]\nunknown = 1\n").is_err());
}
