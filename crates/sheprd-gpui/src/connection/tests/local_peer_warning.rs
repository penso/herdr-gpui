use super::*;

fn warning() -> crate::daemon::LocalPeerWarning {
    crate::daemon::LocalPeerWarning::new(
        crate::daemon::UntrustedEndpoint::DirectoryPermissions,
        "/home/test/herdr/herdr-client.sock".into(),
    )
}

#[test]
fn local_peer_warning_waits_for_the_handshake_and_is_delivered_once() {
    let bridge = bridge();
    bridge.inbox.lock().unwrap().local_peer_warning = Some(warning());
    let connecting = bridge.take_update().unwrap();
    assert!(connecting.local_peer_warning.is_none());
    assert_eq!(
        bridge.inbox.lock().unwrap().local_peer_warning,
        Some(warning())
    );
    {
        let mut state = bridge.inbox.lock().unwrap();
        state.status = ConnectionStatus::AwaitingSnapshot;
        state.dirty = true;
    }
    let connected = bridge.take_update().unwrap();
    assert_eq!(connected.local_peer_warning, Some(warning()));
    assert!(!connected.local_daemon_peer);
    assert!(connected.error.is_none());
    assert!(!connecting.only_surface_changed(&connected));
    bridge.inbox.lock().unwrap().dirty = true;
    assert!(bridge.take_update().unwrap().local_peer_warning.is_none());
}

#[test]
fn local_peer_warning_is_fenced_on_detach_and_reconnect() {
    for reconnect in [false, true] {
        let mut bridge = bridge();
        let old = bridge.inbox.clone();
        old.lock().unwrap().local_peer_warning = Some(warning());
        if reconnect {
            let mut options = ConnectOptions::default();
            options.surface_size.cols = 0;
            bridge.reconnect(options, false, true);
        } else {
            bridge.detach(false);
        }
        {
            let mut stale = old.lock().unwrap();
            stale.local_peer_warning = Some(warning());
            stale.status = ConnectionStatus::Connected;
            stale.dirty = true;
        }
        let current = bridge.take_update().unwrap();
        assert!(current.local_peer_warning.is_none());
        assert!(!current.local_daemon_peer);
    }
}
