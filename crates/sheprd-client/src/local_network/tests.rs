use super::*;
use std::cell::Cell;

#[test]
fn only_a_refused_multicast_send_means_the_permission_is_missing() {
    assert_eq!(access(Ok(12)), LocalNetworkAccess::Allowed);
    // An app macOS can ask for sees no route; a process it cannot ask for,
    // such as a tool in a daemon's pane, sees a broken pipe.
    for kind in [io::ErrorKind::HostUnreachable, io::ErrorKind::BrokenPipe] {
        assert_eq!(access(Err(kind.into())), LocalNetworkAccess::Denied);
    }
    // No IPv4 network, or any other failure, says nothing either way.
    for kind in [
        io::ErrorKind::NetworkUnreachable,
        io::ErrorKind::PermissionDenied,
        io::ErrorKind::AddrNotAvailable,
    ] {
        assert_eq!(access(Err(kind.into())), LocalNetworkAccess::Unknown);
    }
}

/// The first local network operation can be refused while the alert waits
/// for an answer; allowing it then must let the caller continue.
#[test]
fn a_wait_continues_once_the_user_allows_access() {
    let checks = Cell::new(0);
    let access = wait_with(
        Duration::from_secs(5),
        Duration::from_millis(1),
        || {
            checks.set(checks.get() + 1);
            if checks.get() < 3 {
                LocalNetworkAccess::Denied
            } else {
                LocalNetworkAccess::Allowed
            }
        },
        || true,
    );
    assert_eq!(access, LocalNetworkAccess::Allowed);
    assert_eq!(checks.get(), 3);
}

#[test]
fn an_unknown_answer_ends_the_wait_without_guessing() {
    let access = wait_with(
        Duration::from_secs(5),
        Duration::from_millis(1),
        || LocalNetworkAccess::Unknown,
        || true,
    );
    assert_eq!(access, LocalNetworkAccess::Unknown);
}

#[test]
fn a_refusal_that_lasts_ends_the_wait_as_denied() {
    let started = Instant::now();
    let access = wait_with(
        Duration::from_millis(30),
        Duration::from_millis(5),
        || LocalNetworkAccess::Denied,
        || true,
    );
    assert_eq!(access, LocalNetworkAccess::Denied);
    assert!(started.elapsed() >= Duration::from_millis(30));
}

#[test]
fn the_caller_can_stop_waiting() {
    let checks = Cell::new(0);
    let asked = Cell::new(0);
    let access = wait_with(
        Duration::from_secs(60),
        Duration::from_millis(1),
        || {
            checks.set(checks.get() + 1);
            LocalNetworkAccess::Denied
        },
        || {
            asked.set(asked.get() + 1);
            asked.get() <= 2
        },
    );
    assert_eq!(access, LocalNetworkAccess::Denied);
    assert_eq!(checks.get(), 2);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn other_platforms_have_no_local_network_permission() {
    assert_eq!(local_network_access(), LocalNetworkAccess::Allowed);
}
