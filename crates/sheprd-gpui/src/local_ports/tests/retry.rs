//! A worker that stopped is started again by the user, even while the bar's
//! count keeps the scan wanted with the section closed.

use super::*;

/// Ports whose worker stopped while the section was shown and the bar counts.
fn stopped() -> LocalPorts {
    let (mut ports, updates, commands) = LocalPorts::fake();
    ports.counted = true;
    drop((updates, commands));
    assert!(ports.poll(true, Instant::now()));
    assert!(!ports.running());
    ports
}

#[test]
fn a_stopped_worker_stays_stopped_until_the_user_acts() {
    let mut ports = stopped();
    ports.poll(true, Instant::now());
    ports.poll(false, Instant::now());
    assert!(!ports.running());
}

#[test]
fn reopening_the_section_restarts_a_stopped_worker() {
    let mut ports = stopped();
    ports.poll(false, Instant::now());
    assert!(!ports.running());
    ports.poll(true, Instant::now());
    assert!(ports.running());
}

#[test]
fn the_refresh_button_restarts_a_stopped_worker() {
    let mut ports = stopped();
    ports.retry();
    ports.poll(true, Instant::now());
    assert!(ports.running());
}

#[test]
fn a_command_still_hears_at_once_that_the_scan_cannot_run() {
    let mut ports = stopped();
    let requested = Instant::now();
    assert!(!ports.refresh());
    assert!(matches!(ports.fresh_listing(requested), Some(Err(_))));
}
