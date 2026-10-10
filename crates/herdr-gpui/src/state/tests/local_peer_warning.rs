use super::*;

#[test]
fn local_peer_warning_requires_a_full_update_until_consumed() {
    let current = LiveState::default();
    let mut next = current.clone();
    assert!(current.only_surface_changed(&next));

    next.local_peer_warning = Some(crate::daemon::LocalPeerWarning::new(
        crate::daemon::UntrustedEndpoint::DirectoryPermissions,
        "/home/test/herdr/herdr-client.sock".into(),
    ));
    assert!(!current.only_surface_changed(&next));

    next.local_peer_warning.take();
    assert!(current.only_surface_changed(&next));
}
