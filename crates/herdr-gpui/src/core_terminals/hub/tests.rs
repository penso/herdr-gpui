#![allow(clippy::unwrap_used)]

use super::*;
use std::sync::{Condvar, Mutex};

/// Waits up to a few seconds for the hub's attempt to settle.
fn settle(hub: &mut Hub) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(hub.state(), State::Connecting(_)) {
        assert!(Instant::now() < deadline, "the attempt never finished");
        hub.poll();
        std::thread::yield_now();
    }
}

// The upgrade test's join: records each attempt and holds the first until
// the test lets it fail, so a start can be asked for while it is in flight.
static UPGRADE_CALLS: Mutex<Vec<Start>> = Mutex::new(Vec::new());
static UPGRADE_GATE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

fn upgrade_join(start: Start) -> Result<CoreBackend, Error> {
    UPGRADE_CALLS.lock().unwrap().push(start);
    if start == Start::Join {
        let (open, wake) = &UPGRADE_GATE;
        let mut open = open.lock().unwrap();
        while !*open {
            open = wake.wait(open).unwrap();
        }
    }
    Err(Error::NotBundled)
}

#[test]
fn a_start_asked_for_during_a_join_runs_when_the_join_fails() {
    let mut hub = Hub::with_join(upgrade_join);
    hub.connect(Start::Join);
    // A restored tab's join is in flight when the user asks for a terminal.
    hub.connect(Start::IfMissing);
    {
        let (open, wake) = &UPGRADE_GATE;
        *open.lock().unwrap() = true;
        wake.notify_all();
    }
    settle(&mut hub);
    assert_eq!(
        *UPGRADE_CALLS.lock().unwrap(),
        [Start::Join, Start::IfMissing]
    );
    assert!(matches!(
        hub.state(),
        State::Unavailable(error) if matches!(**error, Error::NotBundled)
    ));
}

static RETRY_CALLS: Mutex<Vec<Start>> = Mutex::new(Vec::new());

fn retry_join(start: Start) -> Result<CoreBackend, Error> {
    RETRY_CALLS.lock().unwrap().push(start);
    Err(Error::NotBundled)
}

#[test]
fn a_failed_join_waits_before_joining_again_but_a_start_does_not() {
    let mut hub = Hub::with_join(retry_join);
    hub.connect(Start::Join);
    settle(&mut hub);
    // Restored tabs ask again on every tick; none of these may dial.
    for _ in 0..50 {
        hub.connect(Start::Join);
        hub.poll();
    }
    assert_eq!(*RETRY_CALLS.lock().unwrap(), [Start::Join]);
    hub.connect(Start::IfMissing);
    settle(&mut hub);
    assert_eq!(
        *RETRY_CALLS.lock().unwrap(),
        [Start::Join, Start::IfMissing]
    );
}
