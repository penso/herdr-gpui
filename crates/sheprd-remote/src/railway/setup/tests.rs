#![allow(clippy::unwrap_used)]
use super::*;
use std::{cell::RefCell, fs};

mod public_identity;

/// Stand in for `ssh railway.new` without a Herdr probe that depends on this
/// host: the appended remote command is the installer's probe or its installer
/// (told apart by the URL only the installer contains), answered against a
/// marker file. `installs` says whether "installing" really leaves Herdr.
/// The real scripts are covered by `cloud::install`'s own tests.
fn vm(home: &Path, installs: bool) -> impl Fn() -> Result<Command> {
    let home = home.to_owned();
    let script = format!(
        r#"case "$1" in
*herdr.dev/install.sh*) {install} ;;
*) [ -e "$HOME/herdr" ] && exit 0; exit 3 ;;
esac"#,
        install = if installs {
            r#"touch "$HOME/herdr"; exit 0"#
        } else {
            "exit 0"
        },
    );
    move || {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", &script, "sh"])
            .env_clear()
            .env("HOME", &home)
            .env("PATH", "/usr/bin:/bin");
        Ok(command)
    }
}

#[test]
fn herdr_is_installed_before_a_session_only_when_approved_and_only_if_it_took() {
    let steps = RefCell::new(Vec::new());
    let report = |step: cloud::Step| steps.borrow_mut().push(step);
    let home = tempfile::tempdir().unwrap();
    let installed = || home.path().join("herdr").exists();

    let declined = ensure_herdr(vm(home.path(), true), false, &|| false, &report);
    assert!(matches!(
        declined,
        Err(Error::Cloud(cloud::Error::Install(
            install::Failure::NotInstalled {
                machine: None,
                retry: install::Retry::Allow,
            }
        )))
    ));
    assert!(!installed() && steps.borrow().is_empty());

    // An installer that leaves no Herdr is an error, not a ready device.
    let failed = ensure_herdr(vm(home.path(), false), true, &|| false, &|_| {});
    assert!(matches!(
        failed,
        Err(Error::Cloud(cloud::Error::Install(
            install::Failure::Missing
        )))
    ));

    ensure_herdr(vm(home.path(), true), true, &|| false, &report).unwrap();
    assert!(installed());
    assert_eq!(*steps.borrow(), [cloud::Step::InstallingHerdr]);

    // Already there: the probe passes and the installer is not run again.
    steps.borrow_mut().clear();
    ensure_herdr(vm(home.path(), false), true, &|| false, &report).unwrap();
    assert!(steps.borrow().is_empty());
}

#[test]
fn the_overview_says_where_a_key_will_go_and_which_key_a_device_uses() {
    let state = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let key = home.path().join("id");
    fs::write(&key, "private").unwrap();
    let saved = |account: &Path| SavedDevice {
        provider: CloudProvider::Railway,
        id: "key-a".into(),
        label: "Mine".into(),
        account: account.display().to_string(),
        machine: "railway-old-name".into(),
        session: SESSION.into(),
        enabled: true,
    };
    let devices = vec![saved(&key), saved(&home.path().join("gone"))];
    let overview = overview_in(state.path(), Some(home.path()), devices.clone());
    assert!(!overview.new_key_directory.user);
    // Whether each device's key is still there.
    let present: Vec<_> = overview.devices.iter().map(|d| d.key_present).collect();
    assert_eq!(present, [true, false]);
    assert_eq!(overview.devices[0].key_path(), key);

    fs::create_dir(home.path().join(".ssh")).unwrap();
    let overview = overview_in(state.path(), Some(home.path()), devices);
    assert!(overview.new_key_directory.user);
}

#[test]
fn typed_paths_expand_only_a_leading_home_shortcut() {
    assert_eq!(expand("/abs/key"), Path::new("/abs/key"));
    assert_eq!(expand("a/~/b"), Path::new("a/~/b"));
    if let Some(home) = std::env::var_os("HOME") {
        assert_eq!(expand("~/.ssh/id"), Path::new(&home).join(".ssh/id"));
    }
}
