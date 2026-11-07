//! Updating the daemon on this machine, which runs the copy in the user's
//! home rather than the build the app ships.
use super::*;

#[cfg(not(windows))]
#[gpui::test]
fn this_computers_daemon_is_offered_the_shipped_version(cx: &mut gpui::TestAppContext) {
    let (view, cx) = section::window(cx);
    view.update(cx, |view, _| {
        view.automations.local_version = Some("0.4.0".into());
        // Locally `status` comes from the app's build, so only the daemon's
        // own version can be stale.
        let mut stale = status();
        stale.version = "0.4.0".into();
        stale.daemon.running = true;
        stale.daemon.version = Some("0.3.1".into());
        view.automations.install_for_test(
            wanted(Machine::Local, "This computer"),
            Read::Ready(Arc::new(stale.clone())),
        );
        let host = view.automations.host(&Machine::Local).unwrap();
        assert_eq!(view.automation_update(host), Some("0.4.0".into()));

        stale.daemon.version = Some("0.4.0".into());
        view.automations.install_for_test(
            wanted(Machine::Local, "This computer"),
            Read::Ready(Arc::new(stale)),
        );
        let host = view.automations.host(&Machine::Local).unwrap();
        assert_eq!(view.automation_update(host), None);
    });
}

#[cfg(windows)]
#[gpui::test]
fn this_computer_is_not_offered_an_update_it_cannot_run(cx: &mut gpui::TestAppContext) {
    let (view, cx) = section::window(cx);
    view.update(cx, |view, _| {
        view.automations.local_version = Some("0.4.0".into());
        let mut stale = status();
        stale.daemon.running = true;
        stale.daemon.version = Some("0.3.1".into());
        view.automations.install_for_test(
            wanted(Machine::Local, "This computer"),
            Read::Ready(Arc::new(stale)),
        );
        let host = view.automations.host(&Machine::Local).unwrap();
        assert_eq!(view.automation_update(host), None);
    });
}

/// A `herdr-automate` that logs the path it ran from and its arguments.
#[cfg(unix)]
const FAKE: &str = r#"#!/bin/sh
echo "$0 $*" >> "$HOME/calls"
echo '{"ok":true,"message":"done","run":null}'
"#;

#[cfg(unix)]
#[test]
fn updating_this_computer_runs_the_copy_in_home_not_the_app() {
    use crate::automations::install;
    use std::{fs, sync::atomic::AtomicBool};

    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    let source = dir.path().join("build");
    fs::write(&source, FAKE).unwrap();

    let mut host = Host::new(&sheprd_client::ConnectTarget::Local).unwrap();
    host.env = vec![("HOME".into(), home.display().to_string())];
    let (size, sha256) = install::digest(&source).unwrap();
    let plan = install::Plan {
        platform: install::Platform::local(),
        source,
        version: "0.4.0".into(),
        size,
        sha256,
    };

    let said = install::install(&host, &plan, true, &AtomicBool::new(false)).unwrap();
    assert_eq!(said, "done");

    let link = home.join(".local/bin/herdr-automate");
    assert_eq!(
        fs::read_link(&link).unwrap(),
        home.join(".local/share/herdr-automate/bin/0.4.0/herdr-automate")
    );
    let calls = fs::read_to_string(home.join("calls")).unwrap();
    let expected: String = ["shutdown", "install-service", "start"]
        .map(|command| format!("{} {command} --json\n", link.display()))
        .concat();
    assert_eq!(calls, expected);
}
