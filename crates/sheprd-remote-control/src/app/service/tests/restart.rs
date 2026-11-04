//! Settings' Restart reloads the service, even when nothing changed, and
//! does nothing while remote control is off.

use super::*;

fn on() -> RemoteConfig {
    RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    }
}

fn ran(config: &RemoteConfig, host: &Host) -> (Outcome, Vec<String>) {
    let commands = RefCell::new(Vec::new());
    let outcome = restart(config, host, |step| {
        commands.borrow_mut().push(command(step));
        Ok(())
    })
    .unwrap();
    (outcome, commands.into_inner())
}

#[test]
fn a_restart_reloads_an_unchanged_service() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path());
    reconcile(&on(), &host, |_| unreachable(), |_| Ok(())).unwrap();
    let written = fs::read(&host.file).unwrap();
    let (outcome, commands) = ran(&on(), &host);
    assert_eq!(outcome, Outcome::Started);
    assert!(
        commands.contains(&"systemctl --user restart herdr-remote-control.service".to_owned()),
        "{commands:?}"
    );
    assert_eq!(fs::read(&host.file).unwrap(), written);
}

#[test]
fn a_restart_writes_a_missing_definition_first() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path());
    let (outcome, commands) = ran(&on(), &host);
    assert_eq!(outcome, Outcome::Started);
    assert!(host.file.exists());
    assert!(!commands.is_empty());
}

#[test]
fn a_restart_while_off_runs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path());
    let (outcome, commands) = ran(&RemoteConfig::default(), &host);
    assert_eq!(outcome, Outcome::Unchanged);
    assert!(commands.is_empty());
    assert!(!host.file.exists());
}
