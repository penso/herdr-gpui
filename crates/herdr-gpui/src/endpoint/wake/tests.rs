use super::*;

#[test]
fn a_wall_clock_running_ahead_of_the_monotonic_one_is_a_sleep() {
    let start = Instant::now();
    let wall = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let mut clock = WakeClock::new(start, wall, 0);
    // Both clocks advance together while awake, however long the gap.
    let awake = start + Duration::from_secs(60);
    assert!(!clock.woke(awake, wall + Duration::from_secs(60)));
    // Jitter between them is not a sleep.
    let jitter = awake + Duration::from_secs(1);
    assert!(!clock.woke(jitter, wall + Duration::from_secs(64)));
    // Ten minutes asleep: the monotonic clock moved a moment, the wall clock did not stop.
    let woke = jitter + Duration::from_millis(16);
    assert!(clock.woke(woke, wall + Duration::from_secs(64 + 600)));
    // The next reading is measured from the wake, not the sleep.
    assert!(!clock.woke(
        woke + Duration::from_millis(16),
        wall + Duration::from_secs(664) + Duration::from_millis(16)
    ));
}

#[test]
fn a_wall_clock_set_backwards_is_not_a_sleep() {
    let start = Instant::now();
    let wall = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let mut clock = WakeClock::new(start, wall, 0);
    assert!(!clock.woke(
        start + Duration::from_secs(1),
        wall - Duration::from_secs(3600)
    ));
}

#[gpui::test]
fn waking_redials_dropped_endpoints_now_and_leaves_detached_ones(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        view.reconcile_catalog(
            vec![
                crate::endpoint::tests::host("down", true),
                crate::endpoint::tests::host("parked", true),
            ],
            Vec::new(),
            cx,
        );
        let now = Instant::now();
        let later = now + Duration::from_secs(30);
        for endpoint in &mut view.endpoints {
            endpoint.connection.handle = None;
            endpoint.retry_at = later;
        }
        view.endpoints[2].detached = true;
        let wall = SystemTime::now();
        view.wake = WakeClock::new(now, wall, 0);

        assert!(
            !view.recover_connections(now + Duration::from_secs(1), wall + Duration::from_secs(1))
        );
        assert!(view.endpoints.iter().all(|e| e.retry_at == later));

        let woke = now + Duration::from_secs(2);
        assert!(view.recover_connections(woke, wall + Duration::from_secs(600)));
        assert_eq!(view.endpoints[1].retry_at, woke);
        // A detached endpoint is never dialled by the poll loop, whatever its time.
        assert!(view.endpoints[2].detached);
    });
}

#[test]
fn a_network_change_recovers_at_once_and_a_burst_recovers_once_more_when_settled() {
    let start = Instant::now();
    let wall = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let mut clock = WakeClock::new(start, wall, 0);
    let at = |ms: u64| {
        (
            start + Duration::from_millis(ms),
            wall + Duration::from_millis(ms),
        )
    };
    let (instant, wall_now) = at(16);
    assert_eq!(clock.due(instant, wall_now, 0), None);
    let (instant, wall_now) = at(100);
    assert_eq!(clock.due(instant, wall_now, 1), Some(Trigger::Network));
    // Two more changes while the path settles are held back, not dropped.
    let (instant, wall_now) = at(400);
    assert_eq!(clock.due(instant, wall_now, 2), None);
    let (instant, wall_now) = at(700);
    assert_eq!(clock.due(instant, wall_now, 3), None);
    let (instant, wall_now) = at(1_099);
    assert_eq!(clock.due(instant, wall_now, 3), None);
    let (instant, wall_now) = at(1_100);
    assert_eq!(clock.due(instant, wall_now, 3), Some(Trigger::Network));
    // Settled: nothing more until the next change.
    let (instant, wall_now) = at(5_000);
    assert_eq!(clock.due(instant, wall_now, 3), None);
}

#[test]
fn a_sleep_recovers_even_right_after_a_network_change() {
    let start = Instant::now();
    let wall = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let mut clock = WakeClock::new(start, wall, 0);
    let changed = start + Duration::from_millis(100);
    assert_eq!(
        clock.due(changed, wall + Duration::from_millis(100), 1),
        Some(Trigger::Network)
    );
    // The monotonic clock stops in sleep, so the wake reads only moments later.
    let woke = changed + Duration::from_millis(16);
    assert_eq!(
        clock.due(woke, wall + Duration::from_secs(600), 1),
        Some(Trigger::Sleep)
    );
    // A change reported just after the wake waits out the settling gap, then
    // recovers again: Wi-Fi often joins only after the machine is up.
    let next = woke + Duration::from_millis(16);
    assert_eq!(
        clock.due(
            next,
            wall + Duration::from_secs(600) + Duration::from_millis(16),
            2
        ),
        None
    );
    let settled = woke + Duration::from_secs(1);
    assert_eq!(
        clock.due(settled, wall + Duration::from_secs(601), 2),
        Some(Trigger::Network)
    );
}
