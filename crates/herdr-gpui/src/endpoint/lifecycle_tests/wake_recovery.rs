use super::*;
use crate::endpoint::wake::Trigger;

fn ssh() -> ConnectTarget {
    ConnectTarget::Ssh {
        target: "user@host".into(),
        session: "default".into(),
    }
}

#[test]
fn recovering_starts_the_backoff_over_and_keeps_a_live_link() {
    let (mut endpoint, _server) = connected_endpoint("ssh:test");
    endpoint.connection.target = ssh();
    endpoint.poll(Instant::now());
    assert!(endpoint.live.status.is_connected());
    // Earned across dark wakes, each too short to count as a stable connection.
    endpoint.attempts = 8;
    let later = Instant::now() + Duration::from_secs(30);
    endpoint.retry_at = later;

    assert!(endpoint.recover(Trigger::Sleep, Instant::now()));

    assert_eq!(endpoint.attempts, 0);
    assert_eq!(endpoint.retry_delay(), Duration::from_millis(500));
    // The link is asked to prove itself, not dropped.
    assert!(
        endpoint
            .connection
            .handle
            .as_ref()
            .is_some_and(|handle| !handle.is_disconnected())
    );
    assert_eq!(endpoint.retry_at, later);
}

#[test]
fn recovering_redials_a_remote_attempt_still_dialing_but_not_a_local_one() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "hg-wake-{}-{}.sock",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    // Never accepted: the attempt stays dialing, as one over a dead path does.
    let _listener = UnixListener::bind(&path).unwrap();
    for (target, redialed) in [(ConnectTarget::Socket(path.clone()), false), (ssh(), true)] {
        let mut endpoint = Endpoint::new(
            "test".into(),
            "test".into(),
            ConnectTarget::Socket(path.clone()),
            true,
        );
        endpoint.connect(ConnectOptions::default(), false);
        endpoint.connection.target = target;
        assert!(!endpoint.live.status.is_connected());
        let handle = endpoint.connection.handle.clone().unwrap();
        endpoint.attempts = 8;
        let now = Instant::now();

        assert!(endpoint.recover(Trigger::NetworkRestored, now));

        assert_eq!(endpoint.attempts, 0);
        assert_eq!(handle.is_disconnected(), redialed);
        assert_eq!(endpoint.connection.handle.is_none(), redialed);
        if redialed {
            assert_eq!(endpoint.retry_at, now);
        }
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn recovering_dials_a_dropped_endpoint_at_once() {
    let mut endpoint = Endpoint::new("ssh:test".into(), "test".into(), ssh(), true);
    endpoint.attempts = 8;
    let now = Instant::now();
    endpoint.retry_at = now + Duration::from_secs(30);

    assert!(endpoint.recover(Trigger::NetworkRestored, now));

    assert_eq!(endpoint.retry_at, now);
    assert_eq!(endpoint.attempts, 0);
}

/// The worker can report a link up just before a recovery, in the same poll
/// that has not yet taken the update: that link is live, so it is pinged and
/// kept, not abandoned and redialed.
#[test]
fn recovering_keeps_a_link_whose_connection_is_not_yet_polled() {
    let (mut endpoint, _server) = connected_endpoint("ssh:test");
    endpoint.connection.target = ssh();
    // As before the poll that takes the worker's Connected update.
    endpoint.live.status = ConnectionStatus::Connecting;
    endpoint.connection.inbox.lock().unwrap().dirty = true;
    assert_eq!(endpoint.connection.reports_connected(), Some(true));
    let handle = endpoint.connection.handle.clone().unwrap();
    let later = Instant::now() + Duration::from_secs(30);
    endpoint.retry_at = later;

    assert!(endpoint.recover(Trigger::Sleep, Instant::now()));

    assert!(!handle.is_disconnected());
    assert!(endpoint.connection.handle.is_some());
    assert_eq!(endpoint.retry_at, later);
    endpoint.poll(Instant::now());
    assert!(endpoint.live.status.is_connected());
}

fn dialing_ssh_endpoint() -> (Endpoint, PathBuf, UnixListener) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "hg-wake-dial-{}-{}.sock",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    // Never accepted: the attempt stays dialing.
    let listener = UnixListener::bind(&path).unwrap();
    let mut endpoint = Endpoint::new(
        "ssh:test".into(),
        "test".into(),
        ConnectTarget::Socket(path.clone()),
        true,
    );
    endpoint.connect(ConnectOptions::default(), false);
    endpoint.connection.target = ssh();
    assert!(!endpoint.live.status.is_connected());
    (endpoint, path, listener)
}

/// While the event reader holds the inbox, whether the attempt came up is not
/// known, so the recovery waits for a later poll rather than guess.
#[test]
fn recovering_waits_while_the_worker_report_cannot_be_read() {
    let (mut endpoint, path, _listener) = dialing_ssh_endpoint();
    let handle = endpoint.connection.handle.clone().unwrap();
    endpoint.recovery = Some(Trigger::Sleep);
    let inbox = endpoint.connection.inbox.clone();
    let busy = inbox.lock().unwrap();

    assert!(!endpoint.recover(Trigger::Sleep, Instant::now()));
    endpoint.poll(Instant::now());

    assert!(!handle.is_disconnected());
    assert_eq!(endpoint.recovery, Some(Trigger::Sleep));
    drop(busy);
    endpoint.poll(Instant::now());
    assert!(handle.is_disconnected());
    assert_eq!(endpoint.recovery, None);
    std::fs::remove_file(path).unwrap();
}

/// A path change also comes for changes no connection minds, such as a VPN
/// or an interface attribute, so it neither abandons a dial (which may be
/// waiting on a security key tap) nor starts the backoff over.
#[test]
fn a_path_change_keeps_a_dial_and_its_backoff() {
    let (mut endpoint, path, _listener) = dialing_ssh_endpoint();
    let handle = endpoint.connection.handle.clone().unwrap();
    endpoint.attempts = 8;

    assert!(endpoint.recover(Trigger::PathChanged, Instant::now()));

    assert!(!handle.is_disconnected());
    assert!(endpoint.connection.handle.is_some());
    assert_eq!(endpoint.attempts, 8);
    std::fs::remove_file(path).unwrap();

    let mut dropped = Endpoint::new("ssh:test".into(), "test".into(), ssh(), true);
    let later = Instant::now() + Duration::from_secs(30);
    dropped.retry_at = later;
    assert!(dropped.recover(Trigger::PathChanged, Instant::now()));
    assert_eq!(dropped.retry_at, later);
}

/// A host that refused for a reason only the user can fix keeps its long
/// wait across a wake: the wake fixed nothing about the host key or sign-in.
#[test]
fn recovering_leaves_a_host_that_needs_the_user_on_its_wait() {
    let mut endpoint = Endpoint::new("ssh:test".into(), "test".into(), ssh(), true);
    endpoint.live.ssh_failure = Some(SshFailure::Auth);
    let later = Instant::now() + MAX_RETRY_DELAY;
    endpoint.retry_at = later;

    assert!(endpoint.recover(Trigger::Sleep, Instant::now()));

    assert_eq!(endpoint.retry_at, later);
}
