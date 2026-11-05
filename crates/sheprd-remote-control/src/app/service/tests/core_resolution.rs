use super::*;

#[test]
fn a_bad_core_session_does_not_block_service_reconciliation() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path());
    let on = RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    };
    let mut ran = Vec::new();
    let resolve = || {
        sheprd_core_client::Endpoint::resolve(|key| {
            (key == "HC_SESSION").then(|| "bad/name".into())
        })
    };
    let outcome = reconcile_with_core(
        &on,
        &host,
        |_| unreachable(),
        |step| {
            ran.push(command(step));
            Ok(())
        },
        resolve,
    )
    .unwrap();
    assert_eq!(outcome, Outcome::Installed);
    assert!(host.file.is_file());
    assert!(ran.iter().any(|step| step.contains("restart")));
    ran.clear();
    let outcome = reconcile_with_core(
        &on,
        &host,
        |_| unreachable(),
        |step| {
            ran.push(command(step));
            Ok(())
        },
        resolve,
    )
    .unwrap();
    assert_eq!(outcome, Outcome::Started);
    assert!(ran.iter().any(|step| step.contains("start")));
}

#[test]
fn a_missing_core_home_leaves_discovery_to_the_service() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path());
    let on = RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    };
    assert_eq!(
        reconcile_with_core(
            &on,
            &host,
            |_| unreachable(),
            |_| Ok(()),
            || Err(sheprd_core_client::Error::NoHome)
        )
        .unwrap(),
        Outcome::Installed
    );
    assert!(!fs::read_to_string(host.file).unwrap().contains("--core"));
}
