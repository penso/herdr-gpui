use super::*;

#[test]
fn local_peer_warning_becomes_a_toast_even_when_notifications_are_off() {
    let mut endpoint = Endpoint::new(LOCAL.into(), "Local".into(), ConnectTarget::Local, true);
    {
        let mut state = endpoint.connection.inbox.lock().unwrap();
        state.local_peer_warning = Some(crate::daemon::LocalPeerWarning::new(
            crate::daemon::UntrustedEndpoint::DirectoryPermissions,
            "/home/test/herdr/herdr-client.sock".into(),
        ));
    }
    endpoint.poll(Instant::now());
    assert!(endpoint.toasts.entries.is_empty());
    {
        let mut state = endpoint.connection.inbox.lock().unwrap();
        state.status = ConnectionStatus::Connected;
        state.dirty = true;
    }
    endpoint.poll(Instant::now());
    assert!(endpoint.live.status.is_connected());
    assert_eq!(endpoint.toasts.entries.len(), 1);
    assert!(endpoint.live.local_peer_warning.is_none());
    endpoint.connection.inbox.lock().unwrap().dirty = true;
    endpoint.poll(Instant::now());
    assert_eq!(endpoint.toasts.entries.len(), 1);

    let mut endpoints = [endpoint];
    let config = crate::config::NotificationConfig {
        enabled: false,
        ..Default::default()
    };
    crate::notifications::tick(&mut endpoints, 0, config, true, true, None, Instant::now());
    assert!(
        !endpoints[0].toasts.entries[0].1.visible,
        "wait for open menus"
    );
    crate::notifications::tick(&mut endpoints, 0, config, false, true, None, Instant::now());
    assert!(endpoints[0].toasts.entries[0].1.visible);
    endpoints[0].stop();
    assert!(endpoints[0].toasts.entries.is_empty());
}
