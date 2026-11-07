use super::*;
use crate::control::hosts::{Enroll, Enrolled};

fn enrolled() -> Enrolled {
    Enrolled {
        device: DeviceId("d1".into()),
        computer: "host".into(),
        fingerprint: "F".into(),
        origins: Vec::new(),
        app: None,
    }
}

fn failed() -> crate::app::Error {
    Error::Unexpected.into()
}

#[test]
fn settings_change_only_once_the_phone_is_added() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("herdr/config-gpui.toml");
    let local = dir.path().join("herdr/config-gpui.local.toml");
    // A running Sheprd acts on a write, so neither step may see one.
    enroll_then_keep(
        Some(&path),
        dir.path(),
        |config| {
            assert!(config.enabled);
            assert!(!local.exists());
            Ok(())
        },
        || {
            assert!(!local.exists());
            Ok(enrolled())
        },
    )
    .unwrap();
    assert!(
        fs::read_to_string(&local)
            .unwrap()
            .contains("enabled = true")
    );
}

#[test]
fn a_failed_pairing_leaves_a_host_without_settings_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("herdr/config-gpui.toml");
    let not_started = enroll_then_keep(
        Some(&path),
        dir.path(),
        |_| Err(failed()),
        || panic!("the phone is not added to a daemon that did not start"),
    );
    assert!(not_started.is_err());
    let not_added = enroll_then_keep(Some(&path), dir.path(), |_| Ok(()), || Err(failed()));
    assert!(not_added.is_err());
    assert!(!dir.path().join("herdr").exists());
}

#[test]
fn broken_settings_stop_pairing_before_the_daemon_starts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    fs::write(&path, "[remote\n").unwrap();
    let result = enroll_then_keep(
        Some(&path),
        dir.path(),
        |_| panic!("the daemon is not started"),
        || panic!("the phone is not added"),
    );
    assert!(matches!(
        result,
        Err(crate::app::Error::Enroll(Error::Config { .. }))
    ));
}

#[test]
fn a_host_with_no_sheprd_files_pairs_even_when_settings_cannot_be_written() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the settings' folder would be.
    fs::write(dir.path().join("herdr"), "").unwrap();
    let path = dir.path().join("herdr/config-gpui.toml");
    keep_settings(&path, State::Fresh, dir.path(), &enrolled()).unwrap();
}

#[test]
fn a_host_with_sheprd_fails_when_its_settings_cannot_be_written() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("herdr"), "").unwrap();
    let path = dir.path().join("herdr/config-gpui.toml");
    let result = keep_settings(&path, State::Overrides, dir.path(), &enrolled());
    assert!(matches!(result, Err(crate::app::Error::Io { .. })));
}

#[test]
fn planning_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    let plan = plan_settings(&path).unwrap();
    assert!(plan.config.enabled);
    assert_eq!(plan.state, State::Fresh);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn settings_held_by_sheprd_are_left_until_its_save_is_done() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    let local = dir.path().join("config-gpui.local.toml");
    // Sheprd holds its lock while it saves or moves its settings.
    let lock = fs::File::create(dir.path().join("config-gpui.lock")).unwrap();
    lock.lock().unwrap();
    assert!(matches!(
        enable(&path),
        Err(crate::app::Error::Enroll(Error::SettingsLocked { .. }))
    ));
    assert!(!local.exists());
    fs::write(&local, "theme = \"dark\"\n").unwrap();
    drop(lock);
    assert!(enable(&path).unwrap().enabled);
    // The save Sheprd finished is kept and turned on.
    assert_eq!(
        fs::read_to_string(&local).unwrap(),
        "theme = \"dark\"\n\n[remote]\nenabled = true\n"
    );
}

#[cfg(unix)]
#[test]
fn a_link_to_nothing_is_not_replaced_by_new_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    let local = dir.path().join("config-gpui.local.toml");
    let target = dir.path().join("dotfiles/config-gpui.local.toml");
    std::os::unix::fs::symlink(&target, &local).unwrap();
    assert!(enable(&path).is_err());
    assert_eq!(fs::read_link(&local).unwrap(), target);
}

#[cfg(unix)]
#[test]
fn overrides_behind_a_link_are_changed_where_they_live() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config-gpui.toml");
    let local = dir.path().join("config-gpui.local.toml");
    let target = dir.path().join("dotfiles.toml");
    fs::write(&target, "theme = \"dark\"\n").unwrap();
    std::os::unix::fs::symlink(&target, &local).unwrap();
    assert!(enable(&path).unwrap().enabled);
    assert_eq!(fs::read_link(&local).unwrap(), target);
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "theme = \"dark\"\n\n[remote]\nenabled = true\n"
    );
}

#[test]
fn a_daemon_that_drops_a_web_app_fails_the_pairing() {
    let dir = tempfile::tempdir().unwrap();
    let input = |app: Option<&str>| Input {
        enroll: Enroll {
            name: "iPhone".into(),
            public_key: "KEY".into(),
            via: "MacBook Pro".into(),
            replace: None,
            app: app.map(str::to_owned),
        },
        apns: None,
    };
    let web = input(Some("https://mac.tail1234.ts.net:7443"));
    // Older daemons answer without `app`, which reads as none.
    let older = Ok(Response::Enrolled(enrolled()));
    assert!(matches!(
        read_answer(&web, dir.path(), older),
        Err(crate::app::Error::Enroll(Error::Outdated))
    ));
    let current = Enrolled {
        app: web.enroll.app.clone(),
        ..enrolled()
    };
    assert_eq!(
        read_answer(&web, dir.path(), Ok(Response::Enrolled(current.clone()))).unwrap(),
        current
    );
    // The iOS app sends none, so an older daemon still serves it.
    let native = Ok(Response::Enrolled(enrolled()));
    assert_eq!(
        read_answer(&input(None), dir.path(), native).unwrap(),
        enrolled()
    );
}
