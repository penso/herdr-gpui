//! The orchestrator's workers share a few SSH connections per host, so many
//! projects on one host stay under sshd's `MaxStartups`; a connection sshd
//! dropped before key exchange is tried once more. No SSH server is needed:
//! the slots are exercised directly, with closures standing in for scripts.
use super::*;
use crate::ssh_slots::Slots;
use std::{
    process::ExitStatus,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    time::{Duration, Instant},
};

const NEVER: Duration = Duration::from_secs(5);

fn exited(code: u8) -> ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(i32::from(code) << 8)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(u32::from(code))
    }
}

/// A host script whose `ssh` ended with `code` after printing `stderr`.
fn script_failed(code: u8, stderr: &str) -> Error {
    Error::Script {
        operation: "Reading the repository",
        source: herdr_client::Error::ScriptExit {
            status: exited(code),
            stderr: stderr.into(),
        },
    }
}

fn dropped() -> Error {
    script_failed(
        255,
        "kex_exchange_identification: read: Connection reset by peer\nConnection reset by 100.64.0.1 port 22",
    )
}

/// Waits, for at most [`NEVER`], until `ready` holds.
fn until(ready: impl Fn() -> bool) {
    let deadline = Instant::now() + NEVER;
    while !ready() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn a_full_host_waits_while_another_host_goes_on() {
    let slots = Slots::new(2);
    let never = AtomicBool::new(false);
    // Set before anything is asserted, so a wait that should not happen ends
    // the test instead of hanging it.
    let give_up = AtomicBool::new(false);
    let first = slots.take("a", &never).unwrap();
    let _second = slots.take("a", &never).unwrap();
    let (third, third_taken) = mpsc::channel();
    let (other, other_taken) = mpsc::channel();
    let (third_waited, other_host, third_later) = std::thread::scope(|scope| {
        scope.spawn(|| third.send(slots.take("a", &give_up).is_ok()).unwrap());
        let third_waited =
            third_taken.recv_timeout(Duration::from_millis(200)) == Err(RecvTimeoutError::Timeout);
        scope.spawn(|| {
            let both = [slots.take("b", &give_up), slots.take("b", &give_up)];
            other.send(both.iter().all(Result::is_ok)).unwrap();
        });
        let other_host = other_taken.recv_timeout(NEVER);
        drop(first);
        let third_later = third_taken.recv_timeout(NEVER);
        give_up.store(true, Ordering::Relaxed);
        (third_waited, other_host, third_later)
    });
    assert!(third_waited, "a third connection to a full host must wait");
    assert_eq!(other_host, Ok(true), "another host has slots of its own");
    assert_eq!(
        third_later,
        Ok(true),
        "a freed slot goes to the waiting worker"
    );
}

#[test]
fn many_projects_never_open_more_than_the_limit_per_host() {
    const LIMIT: usize = 3;
    const PROJECTS: usize = 12;
    let slots = Slots::new(LIMIT);
    let never = AtomicBool::new(false);
    let active = [AtomicUsize::new(0), AtomicUsize::new(0)];
    let peak = [AtomicUsize::new(0), AtomicUsize::new(0)];
    let both = AtomicUsize::new(0);
    let both_peak = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for project in 0..PROJECTS * 2 {
            let (host, index) = if project % 2 == 0 { ("a", 0) } else { ("b", 1) };
            let (slots, never, active, peak) = (&slots, &never, &active, &peak);
            let (both, both_peak) = (&both, &both_peak);
            scope.spawn(move || {
                slots
                    .run(Some(host), Duration::ZERO, never, || {
                        let now = active[index].fetch_add(1, Ordering::SeqCst) + 1;
                        peak[index].fetch_max(now, Ordering::SeqCst);
                        let all = both.fetch_add(1, Ordering::SeqCst) + 1;
                        both_peak.fetch_max(all, Ordering::SeqCst);
                        // The first scripts hold their slots until both hosts
                        // are full at once, which they can be only when each
                        // host has its own limit.
                        until(|| both_peak.load(Ordering::SeqCst) == LIMIT * 2);
                        both.fetch_sub(1, Ordering::SeqCst);
                        active[index].fetch_sub(1, Ordering::SeqCst);
                        Ok(())
                    })
                    .unwrap();
            });
        }
    });
    assert_eq!(peak[0].load(Ordering::SeqCst), LIMIT);
    assert_eq!(peak[1].load(Ordering::SeqCst), LIMIT);
    assert_eq!(both_peak.load(Ordering::SeqCst), LIMIT * 2);
}

#[test]
fn a_worker_waiting_for_a_slot_stops_when_cancelled() {
    let slots = Slots::new(1);
    let never = AtomicBool::new(false);
    let _held = slots.take("a", &never).unwrap();
    let cancelled = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let waiting = scope.spawn(|| slots.take("a", &cancelled).map(drop));
        std::thread::sleep(Duration::from_millis(50));
        cancelled.store(true, Ordering::Relaxed);
        assert!(matches!(waiting.join().unwrap(), Err(Error::Cancelled)));
    });
}

#[test]
fn a_connection_dropped_before_key_exchange_is_tried_once_more() {
    let slots = Slots::new(1);
    let never = AtomicBool::new(false);
    let calls = AtomicUsize::new(0);
    let result = slots.run(Some("a"), Duration::ZERO, &never, || {
        match calls.fetch_add(1, Ordering::SeqCst) {
            0 => Err(dropped()),
            _ => Ok(7),
        }
    });
    assert_eq!(result.unwrap(), 7);
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    // Only once: a host that keeps dropping connections fails as before.
    calls.store(0, Ordering::SeqCst);
    let result = slots.run(Some("a"), Duration::ZERO, &never, || {
        calls.fetch_add(1, Ordering::SeqCst);
        Err::<(), _>(dropped())
    });
    assert!(matches!(result, Err(Error::Script { .. })));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    // Every slot was given back.
    drop(slots.take("a", &never).unwrap());
}

#[test]
fn other_failures_and_local_scripts_run_once() {
    let slots = Slots::new(1);
    let never = AtomicBool::new(false);
    let calls = AtomicUsize::new(0);
    let refused = slots.run(Some("a"), Duration::ZERO, &never, || {
        calls.fetch_add(1, Ordering::SeqCst);
        Err::<(), _>(script_failed(255, "user@a: Permission denied (publickey)."))
    });
    assert!(matches!(refused, Err(Error::Script { .. })));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // A local script takes no slot, so it runs even while an SSH host is
    // full, and is never retried.
    let _held = slots.take("a", &never).unwrap();
    calls.store(0, Ordering::SeqCst);
    let local = slots.run(None, Duration::ZERO, &never, || {
        calls.fetch_add(1, Ordering::SeqCst);
        Err::<(), _>(dropped())
    });
    assert!(matches!(local, Err(Error::Script { .. })));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_cancelled_worker_skips_the_retry() {
    let slots = Slots::new(1);
    let cancelled = AtomicBool::new(false);
    let calls = AtomicUsize::new(0);
    let result = slots.run(Some("a"), NEVER, &cancelled, || {
        calls.fetch_add(1, Ordering::SeqCst);
        cancelled.store(true, Ordering::Relaxed);
        Err::<(), _>(dropped())
    });
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
