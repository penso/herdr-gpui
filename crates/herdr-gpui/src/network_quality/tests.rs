#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::{
    LATENCY_PROBES, NetworkQuality, RELEASE, Reading, WINDOW,
    probe::{self, Outcome},
};
use std::{
    io::ErrorKind,
    net::{SocketAddr, TcpListener},
    time::{Duration, Instant, SystemTime},
};

/// A fixed wall-clock time, so expiry tests do not depend on the real one.
fn start() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

fn seconds(count: u64) -> Duration {
    Duration::from_secs(count)
}

fn reply(milliseconds: u64) -> Outcome {
    Outcome::Reply(Duration::from_millis(milliseconds))
}

fn lost() -> Outcome {
    Outcome::Lost(ErrorKind::TimedOut)
}

#[test]
fn a_listening_port_answers() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let target = listener.local_addr().expect("address");
    let outcome = probe::run(target, Duration::from_secs(2));
    assert!(outcome.round_trip().is_some(), "{outcome:?}");
}

#[test]
fn a_closed_port_is_lost() {
    // The listener is dropped so nothing listens there, while the connection
    // it accepted keeps the port in use: a parallel test cannot be given it
    // for its own listener, and a probe cannot connect to itself through it.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let target: SocketAddr = listener.local_addr().expect("address");
    let _client = std::net::TcpStream::connect(target).expect("connect");
    let _held = listener.accept().expect("accept");
    drop(listener);
    // Windows retries a refused connect for about two seconds before
    // reporting it, so the kind may be a refusal or a timeout; either way
    // the probe is lost.
    let outcome = probe::run(target, Duration::from_secs(5));
    assert!(
        matches!(
            outcome,
            Outcome::Lost(ErrorKind::ConnectionRefused | ErrorKind::TimedOut)
        ),
        "{outcome:?}"
    );
}

#[test]
fn latency_is_the_median_of_recent_answers() {
    let mut reading = Reading::default();
    assert_eq!(reading.latency(), None);
    // Older than the latency probes, so it does not count.
    reading.apply(start(), reply(900));
    for milliseconds in [30, 500, 40, 35] {
        reading.apply(start(), reply(milliseconds));
    }
    assert_eq!(reading.latency(), Some(Duration::from_millis(40)));
}

#[test]
fn latency_is_none_when_recent_probes_were_all_lost() {
    let mut reading = Reading::default();
    reading.apply(start(), reply(30));
    for _ in 0..LATENCY_PROBES {
        reading.apply(start(), lost());
    }
    assert_eq!(reading.latency(), None);
}

#[test]
fn loss_and_range_cover_the_window() {
    let mut reading = Reading::default();
    assert_eq!(reading.loss_percent(), None);
    reading.apply(start(), reply(20));
    reading.apply(start(), lost());
    reading.apply(start(), reply(80));
    reading.apply(start(), reply(50));
    assert_eq!(reading.loss_percent(), Some(25.));
    assert_eq!(
        reading.range(),
        Some((Duration::from_millis(20), Duration::from_millis(80)))
    );
}

#[test]
fn loss_forgets_probes_older_than_the_window() {
    let mut reading = Reading::default();
    reading.apply(start(), lost());
    for _ in 0..WINDOW {
        reading.apply(start(), reply(30));
    }
    assert_eq!(reading.probes(), WINDOW);
    assert_eq!(reading.loss_percent(), Some(0.));
}

#[test]
fn details_name_the_target_and_the_loss() {
    let mut reading = Reading::default();
    reading.apply(start(), reply(31));
    reading.apply(start(), Outcome::Lost(ErrorKind::TimedOut));
    let text = super::render::details(&reading);
    assert!(text.contains("1.1.1.1:443"), "{text}");
    assert!(text.contains("no connection"), "{text}");
    assert!(text.contains("Median 31 ms"), "{text}");
    assert!(text.contains("Loss 50% of 2 probes"), "{text}");
}

#[test]
fn hiding_stops_and_forgets() {
    let mut quality = NetworkQuality::default();
    let tick = Instant::now();
    assert!(!quality.poll(false, tick, start()), "nothing was shown");
    quality.reading.apply(start(), reply(30));
    quality.started = true;
    assert!(quality.poll(false, tick, start()), "the reading disappears");
    assert!(quality.reading.latest().is_none());
}

/// A window still holding `false` from before a settings change must not
/// stop the probes another window asks for on the same ticks.
#[test]
fn a_stale_window_does_not_stop_probes_another_wants() {
    // Marked started with no worker, so the test spawns no probing thread.
    let mut quality = NetworkQuality {
        started: true,
        ..Default::default()
    };
    quality.reading.apply(start(), reply(30));
    let tick = Instant::now();
    quality.poll(true, tick, start());
    assert!(!quality.poll(false, tick, start()), "the reading stays");
    assert!(quality.started && quality.reading.latest().is_some());
    let later = tick + Duration::from_millis(16);
    assert!(!quality.poll(false, later, start()));
    quality.poll(true, later, start());
    assert!(quality.reading.latest().is_some());
}

#[test]
fn probes_stop_once_no_window_has_asked_for_the_release_time() {
    let mut quality = NetworkQuality {
        started: true,
        ..Default::default()
    };
    quality.reading.apply(start(), reply(30));
    let tick = Instant::now();
    quality.poll(true, tick, start());
    let before = tick + RELEASE - Duration::from_millis(1);
    assert!(
        !quality.poll(false, before, start()),
        "still within the release"
    );
    assert!(quality.poll(false, tick + RELEASE, start()), "released");
    assert!(!quality.started && quality.reading.latest().is_none());
}

#[test]
fn probes_expire_after_the_span() {
    let mut reading = Reading::default();
    reading.apply(start(), reply(30));
    reading.apply(start() + seconds(240), reply(40));
    assert!(
        !reading.expire(start() + super::SPAN),
        "both are within the span"
    );
    assert!(reading.expire(start() + super::SPAN + seconds(1)));
    assert_eq!(reading.probes(), 1);
    assert_eq!(reading.latency(), Some(Duration::from_millis(40)));
}

#[test]
fn a_night_asleep_leaves_nothing_shown() {
    let mut reading = Reading::default();
    reading.apply(start(), lost());
    reading.apply(start(), reply(30));
    assert!(reading.expire(start() + seconds(8 * 3600)));
    assert_eq!(reading.latest(), None);
    assert_eq!(reading.loss_percent(), None);
}

#[test]
fn a_clock_set_back_drops_probes_from_the_future() {
    let mut reading = Reading::default();
    reading.apply(start(), reply(30));
    assert!(reading.expire(start() - seconds(60)));
    assert_eq!(reading.probes(), 0);
}
