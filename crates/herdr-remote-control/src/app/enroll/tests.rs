use super::*;
use crate::control::DeviceId;

fn apns() -> Apns {
    Apns {
        config: r#"{"team_id":"T","key_id":"K","topic":"so.pen.herdr.remote"}"#.into(),
        key: "-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----\n".into(),
    }
}

#[test]
fn the_input_is_the_enrollment_with_optional_apns_files() {
    let input: Input = serde_json::from_str(
        r#"{"name":"iPhone","public_key":"KEY","via":"MacBook Pro","replace":"d0"}"#,
    )
    .unwrap();
    assert_eq!(input.enroll.name, "iPhone");
    assert_eq!(input.enroll.replace, Some(DeviceId("d0".into())));
    assert_eq!(input.apns, None);
    let with_apns = Input {
        apns: Some(apns()),
        ..input
    };
    let json = serde_json::to_value(&with_apns).unwrap();
    assert_eq!(json["apns"]["config"], apns().config);
    assert_eq!(serde_json::from_value::<Input>(json).unwrap(), with_apns);
}

#[test]
fn apns_files_never_reach_a_log() {
    let shown = format!("{:?}", apns());
    assert!(!shown.contains("secret"));
    assert!(!shown.contains("team_id"));
}

#[test]
fn apns_files_are_written_privately_once() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(Apns::read(dir.path()), None);
    assert!(write_apns(dir.path(), &apns()).unwrap());
    assert_eq!(Apns::read(dir.path()), Some(apns()));
    // The same files again change nothing, so the daemon is not restarted.
    assert!(!write_apns(dir.path(), &apns()).unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir.path().join("apns")), 0o700);
        assert_eq!(mode(&dir.path().join("apns/AuthKey.p8")), 0o600);
        assert_eq!(mode(&dir.path().join("apns/config.json")), 0o600);
    }
    let huge = Apns {
        key: "k".repeat(MAX_APNS_FILE + 1),
        ..apns()
    };
    assert!(matches!(
        write_apns(dir.path(), &huge),
        Err(super::super::Error::Enroll(Error::ApnsSize))
    ));
}

#[test]
fn a_host_without_herdr_ui_settings_runs_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    let config = enable_settings_at(&path).unwrap();
    assert!(config.enabled);
    assert_eq!(config.port, crate::app::config::DEFAULT_PORT);
    // Nothing is created for an app that is not there.
    assert!(!path.exists());
}

#[test]
fn herdr_ui_settings_on_the_host_are_turned_on_and_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    fs::write(
        &path,
        "# mine\ntheme = \"dark\"\n\n[remote]\nenabled = false\nport = 8443\n",
    )
    .unwrap();
    let config = enable_settings_at(&path).unwrap();
    assert!(config.enabled);
    assert_eq!(config.port, 8443);
    let text = fs::read_to_string(&path).unwrap();
    assert_eq!(
        text,
        "# mine\ntheme = \"dark\"\n\n[remote]\nenabled = true\nport = 8443\n"
    );
    // Already on: the file is left as it is.
    fs::write(&path, "[remote]\nenabled = true # on\n").unwrap();
    assert!(enable_settings_at(&path).unwrap().enabled);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "[remote]\nenabled = true # on\n"
    );
}

#[test]
fn broken_settings_are_reported_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    fs::write(&path, "[remote\n").unwrap();
    assert!(matches!(
        enable_settings_at(&path),
        Err(super::super::Error::Enroll(Error::Config { .. }))
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), "[remote\n");
}
