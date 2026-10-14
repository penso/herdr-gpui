use super::*;

#[test]
fn a_failed_managed_transport_does_not_claim_the_host_is_trusted() {
    let root = tempfile::tempdir().unwrap();
    let binary = root.path().join("tailscale");
    crate::test_executable::write(&binary, "#!/bin/sh\nexit 19\n", 0o700).unwrap();
    let target = SshTarget {
        target: "smoke@fixture.invalid".into(),
        command: crate::SshCommand::try_from(format!("{} ssh", binary.display())).unwrap(),
    };
    assert!(matches!(
        scan_host_key(&target).unwrap(),
        HostKeyScan::Refused(SshFailure::Other)
    ));
}
