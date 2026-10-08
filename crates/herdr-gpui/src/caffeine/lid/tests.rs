#![allow(clippy::unwrap_used)]

use super::*;
use gpui::TestAppContext;
use std::{cell::RefCell, path::Path, rc::Rc};

/// Every fake step appends its name to `log`, so tests read the order back.
fn shell(script: String) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), script]
}

/// Sleep starts allowed; `allowed` stands in for `sudo -l`.
fn fakes(
    dir: &Path,
    allowed: &str,
    disable_sleep: &str,
    install_rule: &str,
    watcher: &str,
) -> Commands {
    let at = |script: &str| shell(format!("cd '{}' && {script}", dir.display()));
    Commands {
        sleep_settings: at("printf ' SleepDisabled\\t\\t0\\n'"),
        allowed: at(allowed),
        disable_sleep: at(disable_sleep),
        install_rule: at(install_rule),
        watcher: at(watcher),
    }
}

fn working(dir: &Path) -> Commands {
    fakes(
        dir,
        "true",
        "echo hold >> log",
        "echo install >> log",
        "read line; echo release >> log",
    )
}

fn log(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("log")).unwrap_or_default()
}

type Reported = Rc<RefCell<Vec<Failure>>>;

fn install(commands: Commands, cx: &mut TestAppContext) -> Reported {
    cx.update(|cx| cx.default_global::<Caffeine>().lid.commands = commands);
    Rc::default()
}

fn want_now(wanted: bool, reported: &Reported, cx: &mut TestAppContext) {
    let reported = reported.clone();
    cx.update(|cx| {
        want(
            wanted,
            move |failure, _| reported.borrow_mut().push(failure),
            cx,
        )
    });
}

fn held(cx: &mut TestAppContext) -> bool {
    cx.read_global::<Caffeine, _>(|caffeine, _| caffeine.lid.held())
}

#[gpui::test]
fn holds_behind_a_watcher_and_releases_when_the_cup_turns_off(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let reported = install(working(dir.path()), cx);

    want_now(true, &reported, cx);
    cx.run_until_parked();
    assert!(held(cx));
    assert_eq!(log(dir.path()), "hold\n");

    want_now(false, &reported, cx);
    cx.run_until_parked();
    assert!(!held(cx));
    assert_eq!(log(dir.path()), "hold\nrelease\n");
    assert!(reported.borrow().is_empty());
}

#[gpui::test]
fn installs_the_rule_only_when_sudo_refuses(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let reported = install(
        fakes(
            dir.path(),
            "test -e rule",
            "echo hold >> log",
            "touch rule && echo install >> log",
            "read line; echo release >> log",
        ),
        cx,
    );

    want_now(true, &reported, cx);
    cx.run_until_parked();
    assert!(held(cx));
    assert_eq!(log(dir.path()), "install\nhold\n");

    want_now(false, &reported, cx);
    cx.run_until_parked();
    want_now(true, &reported, cx);
    cx.run_until_parked();
    assert!(held(cx));
    assert_eq!(log(dir.path()), "install\nhold\nrelease\nhold\n");
    want_now(false, &reported, cx);
    cx.run_until_parked();
}

#[gpui::test]
fn a_cancelled_password_dialog_reports_and_is_not_retried(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let reported = install(
        fakes(
            dir.path(),
            "exit 1",
            "exit 1",
            "echo 'User canceled. (-128)' >&2; exit 1",
            "read line; echo release >> log",
        ),
        cx,
    );

    want_now(true, &reported, cx);
    cx.run_until_parked();
    assert!(!held(cx));
    assert!(!cx.read_global::<Caffeine, _>(|caffeine, _| caffeine.lid.wanted));
    // The watcher still ran its restore, which is harmless here.
    assert_eq!(log(dir.path()), "release\n");
    let reported = reported.borrow();
    assert!(matches!(
        reported.as_slice(),
        [Failure::Hold(Error::LidPasswordCancelled)]
    ));
}

#[gpui::test]
fn other_install_failures_keep_their_detail(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let reported = install(
        fakes(
            dir.path(),
            "exit 1",
            "exit 1",
            "echo 'visudo: syntax error' >&2; exit 1",
            "read line",
        ),
        cx,
    );

    want_now(true, &reported, cx);
    cx.run_until_parked();
    assert!(!held(cx));
    let reported = reported.borrow();
    assert!(matches!(
        reported.as_slice(),
        [Failure::Hold(Error::LidCommandFailed { operation: "allow Herdr to change lid sleep", detail, .. })]
            if detail == "visudo: syntax error"
    ));
}

#[gpui::test]
fn a_failed_restore_tells_the_user_how_to_finish_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let reported = install(
        fakes(
            dir.path(),
            "true",
            "echo hold >> log",
            "exit 1",
            "read line; exit 3",
        ),
        cx,
    );

    want_now(true, &reported, cx);
    cx.run_until_parked();
    want_now(false, &reported, cx);
    cx.run_until_parked();
    assert!(!held(cx));
    let reported = reported.borrow();
    assert!(matches!(
        reported.as_slice(),
        [Failure::Release(error @ Error::LidRelease(status))]
            if status.code() == Some(3) && error.to_string().contains("sudo pmset disablesleep 0")
    ));
}

#[gpui::test]
fn a_change_while_a_command_runs_settles_on_the_latest_wish(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let reported = install(working(dir.path()), cx);

    want_now(true, &reported, cx);
    want_now(false, &reported, cx);
    cx.run_until_parked();
    assert!(!held(cx));
    assert_eq!(log(dir.path()), "hold\nrelease\n");
    assert!(reported.borrow().is_empty());
}

#[test]
fn the_rule_names_only_the_two_pmset_commands_by_uid() {
    assert_eq!(
        rule(501),
        "#501 ALL=(root) NOPASSWD: /usr/bin/pmset disablesleep 1, /usr/bin/pmset disablesleep 0"
    );
    let commands = Commands::default();
    assert_eq!(commands.sleep_settings, ["/usr/bin/pmset", "-g"]);
    assert_eq!(
        commands.allowed,
        [
            "/usr/bin/sudo",
            "-n",
            "-l",
            "/usr/bin/pmset",
            "disablesleep",
            "1"
        ]
    );
    assert_eq!(
        commands.disable_sleep,
        ["/usr/bin/sudo", "-n", "/usr/bin/pmset", "disablesleep", "1"]
    );
    assert!(commands.watcher[2].ends_with("/usr/bin/sudo -n /usr/bin/pmset disablesleep 0"));
}

#[gpui::test]
fn sleep_already_disabled_is_left_as_found(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let mut commands = working(dir.path());
    commands.sleep_settings = shell("printf ' SleepDisabled\\t\\t1\\n'".into());
    let reported = install(commands, cx);

    want_now(true, &reported, cx);
    cx.run_until_parked();
    assert!(held(cx));
    want_now(false, &reported, cx);
    cx.run_until_parked();
    assert!(!held(cx));
    // Neither disabled nor restored: whoever set the flag owns it.
    assert_eq!(log(dir.path()), "");
    assert!(reported.borrow().is_empty());
}

#[gpui::test]
fn a_failing_pmset_does_not_ask_for_the_password_when_the_rule_allows_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let reported = install(
        fakes(
            dir.path(),
            "true",
            "echo 'pmset: busy' >&2; exit 1",
            "echo install >> log",
            "read line; echo release >> log",
        ),
        cx,
    );

    want_now(true, &reported, cx);
    cx.run_until_parked();
    assert!(!held(cx));
    assert_eq!(log(dir.path()), "release\n");
    let reported = reported.borrow();
    assert!(matches!(
        reported.as_slice(),
        [Failure::Hold(Error::LidCommandFailed { operation: "keep the Mac running with the lid closed", detail, .. })]
            if detail == "pmset: busy"
    ));
}

#[gpui::test]
fn a_sudo_that_cannot_start_does_not_ask_for_the_password(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let mut commands = working(dir.path());
    commands.allowed = vec![dir.path().join("missing-sudo").display().to_string()];
    let reported = install(commands, cx);

    want_now(true, &reported, cx);
    cx.run_until_parked();
    assert!(!held(cx));
    assert!(!log(dir.path()).contains("install"));
    let reported = reported.borrow();
    assert!(matches!(
        reported.as_slice(),
        [Failure::Hold(Error::LidCommand {
            operation: "check the closed-lid sudoers rule",
            ..
        })]
    ));
}

#[gpui::test]
fn unreadable_sleep_settings_refuse_the_hold(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let mut commands = working(dir.path());
    commands.sleep_settings = shell("exit 2".into());
    let reported = install(commands, cx);

    want_now(true, &reported, cx);
    cx.run_until_parked();
    assert!(!held(cx));
    assert_eq!(log(dir.path()), "");
    assert!(matches!(
        reported.borrow().as_slice(),
        [Failure::Hold(Error::LidCommandFailed {
            operation: "read the Mac's sleep settings",
            ..
        })]
    ));
}
