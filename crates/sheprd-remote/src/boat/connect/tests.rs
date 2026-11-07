use super::*;

#[test]
fn unsupported_connections_fail_before_loading_settings_or_resuming_sandboxes() {
    assert!(matches!(
        command("", "", &AtomicBool::new(false)),
        Err(Error::Cloud(crate::cloud::Error::Unavailable(_)))
    ));
}
