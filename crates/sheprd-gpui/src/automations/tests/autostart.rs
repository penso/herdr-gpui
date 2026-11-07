use super::*;
use crate::automations::{AUTOSTART_EVERY, HostState};
use std::time::{Duration, Instant};

fn host(running: bool, scripts: bool) -> HostState {
    let mut status = status();
    status.daemon.running = running;
    if !scripts {
        status.scripts.clear();
    }
    let mut host = HostState::new(wanted(Machine::Local, "This computer"));
    host.read = Read::Ready(Arc::new(status));
    host
}

#[test]
fn a_host_with_scripts_and_no_daemon_wants_one() {
    let now = Instant::now();
    assert!(host(false, true).wants_daemon(now));
    assert!(!host(true, true).wants_daemon(now), "already running");
    assert!(!host(false, false).wants_daemon(now), "nothing to run");
    let mut unread = host(false, true);
    unread.read = Read::Unread;
    assert!(!unread.wants_daemon(now));
}

#[test]
fn a_start_that_did_not_take_is_retried_slowly() {
    let now = Instant::now();
    let mut tried = host(false, true);
    tried.autostarted = Some(now);
    assert!(!tried.wants_daemon(now + Duration::from_secs(10)));
    assert!(tried.wants_daemon(now + AUTOSTART_EVERY));
}
