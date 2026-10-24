use super::*;

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

    endpoint.recover(Instant::now());

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

        endpoint.recover(now);

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

    endpoint.recover(now);

    assert_eq!(endpoint.retry_at, now);
    assert_eq!(endpoint.attempts, 0);
}
