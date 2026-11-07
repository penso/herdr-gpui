//! A release app restarts a daemon of another version; a development build
//! leaves a running one alone.

use super::*;

fn installed(dir: &Path, release: Option<&str>) -> Host {
    let host = Host {
        release: release.map(str::to_owned),
        ..host(dir)
    };
    let on = RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    };
    reconcile(&on, &host, |_| unreachable(), |_| Ok(())).unwrap();
    host
}

fn steps(host: &Host, version: &str) -> (Outcome, usize) {
    let on = RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    };
    let ran = RefCell::new(0);
    let outcome = reconcile(
        &on,
        host,
        |_| Ok(status(version)),
        |_| {
            *ran.borrow_mut() += 1;
            Ok(())
        },
    )
    .unwrap();
    (outcome, ran.into_inner())
}

#[test]
fn a_release_restarts_a_daemon_of_another_version() {
    let dir = tempfile::tempdir().unwrap();
    let host = installed(dir.path(), Some("1.2.0"));
    assert_eq!(steps(&host, "1.2.0"), (Outcome::Unchanged, 0));
    let (outcome, ran) = steps(&host, "1.1.0");
    assert_eq!(outcome, Outcome::Started);
    assert!(ran > 0);
}

#[test]
fn a_development_build_keeps_the_running_daemon() {
    let dir = tempfile::tempdir().unwrap();
    let host = installed(dir.path(), None);
    assert_eq!(steps(&host, "1.1.0"), (Outcome::Unchanged, 0));
}
