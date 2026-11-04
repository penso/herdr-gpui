#![allow(clippy::unwrap_used)]
use super::*;
use crate::config::ContainerConfig;

mod idle;
mod live;
mod settings;

fn config(pairs: &[(&str, &str)]) -> ContainerConfig {
    let text = pairs
        .iter()
        .map(|(key, value)| format!("{key} = {value}\n"))
        .collect::<String>();
    toml::from_str(&text).unwrap()
}

#[test]
fn an_empty_table_makes_hardened_defaults() {
    let settings = Settings::resolve(&ContainerConfig::default()).unwrap();
    assert_eq!(settings.engine, None);
    assert_eq!(settings.image, setup::DEFAULT_IMAGE);
    assert_eq!(settings.cpus, "4");
    assert_eq!(settings.memory, "8g");
    assert_eq!(settings.pids, 4096);
    assert_eq!(
        settings.auto_stop, None,
        "containers keep running unless asked"
    );
    assert_eq!(settings.max, 8);
}

#[test]
fn numbers_may_be_written_as_numbers_or_strings() {
    let settings = Settings::resolve(&config(&[
        ("engine", "\"podman\""),
        ("cpus", "1.5"),
        ("memory", "\"512m\""),
        ("pids_limit", "128"),
    ]))
    .unwrap();
    assert_eq!(settings.engine, Some(Engine::Podman));
    assert_eq!(settings.cpus, "1.5");
    assert_eq!(settings.memory, "512m");
    assert_eq!(settings.pids, 128);
}

#[test]
fn values_the_engine_could_misread_are_refused() {
    for (key, value) in [
        ("engine", "\"lxc\""),
        ("image", "\"--privileged\""),
        ("image", "\"ubuntu latest\""),
        ("cpus", "0"),
        ("cpus", "\"four\""),
        ("memory", "\"8gb\""),
        ("memory", "\"-1g\""),
        ("pids_limit", "1"),
        ("auto_stop", "\"1m\""),
        ("max_containers", "0"),
        ("max_containers", "65"),
    ] {
        let error = Settings::resolve(&config(&[(key, value)])).unwrap_err();
        assert!(matches!(error, Error::Field(_)), "{key} = {value}: {error}");
    }
}

#[test]
fn containers_run_with_limits_and_never_with_host_access() {
    let settings = Settings::resolve(&ContainerConfig::default()).unwrap();
    let args = machine::run_args("herdr-calm-otter-1a", "debian@sha256:abc", &settings);
    let joined = args.join(" ");
    for wanted in [
        "--init",
        "--security-opt no-new-privileges",
        "--cpus 4",
        "--memory 8g",
        "--pids-limit 4096",
        "--label dev.herdr-gpui.device=herdr-calm-otter-1a",
        "--volume herdr-calm-otter-1a-home:/home/herdr",
    ] {
        assert!(joined.contains(wanted), "{wanted} in {joined}");
    }
    for refused in [
        "--privileged",
        "docker.sock",
        "--network host",
        "--pid host",
        "/Users",
        "$HOME",
    ] {
        assert!(!joined.contains(refused), "{refused} in {joined}");
    }
    // The image comes last but for the command, so nothing after it is an
    // option, and the keep-alive replaces any entrypoint the image names.
    assert_eq!(
        &args[args.len() - 3..],
        ["sleep", "debian@sha256:abc", "infinity"]
    );
    assert_eq!(args[args.len() - 4], "--entrypoint");
}

#[test]
fn engines_are_saved_by_key() {
    for engine in Engine::ALL {
        assert_eq!(Engine::from_key(engine.key()), Some(engine));
    }
    assert_eq!(Engine::from_key("Docker"), None);
}

#[test]
fn engine_errors_keep_their_last_line_without_controls() {
    let stderr = b"pulling...\nError response from daemon: No such container: x\x1b[0m\n\n";
    assert_eq!(
        engine::detail(stderr),
        "Error response from daemon: No such container: x[0m"
    );
    assert_eq!(engine::detail(b""), "no error output");
}

#[test]
fn a_saved_device_with_an_unknown_engine_is_refused() {
    let error = command("lxc", "herdr-x", &std::sync::atomic::AtomicBool::new(false)).unwrap_err();
    assert!(matches!(error, Error::Field(_)));
}
