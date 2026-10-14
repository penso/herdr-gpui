use super::*;

#[test]
fn invalid_targets_and_sessions_are_rejected_before_the_platform_refusal() {
    let stop = AtomicBool::new(false);
    assert!(matches!(
        connect(&"-oProxyCommand=x".into(), "default", &stop),
        Err(Error::InvalidSshTarget)
    ));
    assert!(matches!(
        connect(&"host".into(), "../escape", &stop),
        Err(Error::InvalidSession)
    ));
    assert!(matches!(
        connect(&"host".into(), "default", &stop),
        Err(Error::SshUnsupported)
    ));
}
