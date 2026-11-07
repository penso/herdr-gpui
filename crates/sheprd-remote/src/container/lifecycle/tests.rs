#![allow(clippy::unwrap_used)]
use super::*;
use std::{sync::mpsc, time::Duration};

const WAIT: Duration = Duration::from_secs(5);

#[test]
fn successful_stop_blocks_retries_until_explicit_resume() {
    let device = Lifecycle::default();
    device.stop(|| Ok(())).unwrap();
    assert!(device.is_suspended());
    assert!(matches!(
        device.start("test", || panic!("must not restart")),
        Err(Error::Suspended(_))
    ));
    device.resume();
    device.start("test", || Ok(())).unwrap();
}

#[test]
fn failed_stop_restores_retries_and_preserves_the_error() {
    let device = Lifecycle::default();
    assert!(matches!(
        device.stop(|| Err(Error::Cancelled)),
        Err(Error::Cancelled)
    ));
    assert!(!device.is_suspended());
    device.start("test", || Ok(())).unwrap();
}

#[test]
fn a_start_already_in_flight_finishes_before_the_stop() {
    let device = Arc::new(Lifecycle::default());
    let (started, starting) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (events, observed) = mpsc::channel();
    let start = {
        let device = device.clone();
        let events = events.clone();
        std::thread::spawn(move || {
            device.start("test", || {
                started.send(()).unwrap();
                released.recv_timeout(WAIT).unwrap();
                events.send("started").unwrap();
                Ok(())
            })
        })
    };
    starting.recv_timeout(WAIT).unwrap();
    let stop = {
        let device = device.clone();
        std::thread::spawn(move || {
            device.stop(|| {
                events.send("stopped").unwrap();
                Ok(())
            })
        })
    };
    release.send(()).unwrap();
    assert_eq!(observed.recv_timeout(WAIT).unwrap(), "started");
    assert_eq!(observed.recv_timeout(WAIT).unwrap(), "stopped");
    start.join().unwrap().unwrap();
    stop.join().unwrap().unwrap();
    assert!(device.is_suspended());
}

#[test]
fn explicit_resume_during_stop_waits_for_stop_to_finish_before_starting() {
    let device = Arc::new(Lifecycle::default());
    let (stopping, stopped) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop = {
        let device = device.clone();
        let finished = finished.clone();
        std::thread::spawn(move || {
            device.stop(|| {
                stopping.send(()).unwrap();
                released.recv_timeout(WAIT).unwrap();
                finished.store(true, std::sync::atomic::Ordering::Release);
                Ok(())
            })
        })
    };
    stopped.recv_timeout(WAIT).unwrap();
    assert!(device.is_suspended());
    // This is the UI-thread operation: it must complete while stop is blocked.
    device.resume();
    let start = {
        let device = device.clone();
        std::thread::spawn(move || {
            device.start("test", || {
                assert!(finished.load(std::sync::atomic::Ordering::Acquire));
                Ok(())
            })
        })
    };
    release.send(()).unwrap();
    stop.join().unwrap().unwrap();
    start.join().unwrap().unwrap();
    assert!(!device.is_suspended());
}

#[test]
fn a_failed_stop_cannot_undo_one_that_succeeded() {
    let device = Lifecycle::default();
    device.stop(|| Ok(())).unwrap();
    assert!(matches!(
        device.stop(|| Err(Error::Cancelled)),
        Err(Error::Cancelled)
    ));
    assert!(device.is_suspended());
    assert!(matches!(
        device.start("test", || panic!("must not restart")),
        Err(Error::Suspended(_))
    ));
}

#[test]
fn a_reconnect_waits_for_a_stop_queued_behind_an_earlier_start() {
    let device = Arc::new(Lifecycle::default());
    let (started, starting) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (events, observed) = mpsc::channel();
    let earlier = {
        let device = device.clone();
        let events = events.clone();
        std::thread::spawn(move || {
            device.start("test", || {
                started.send(()).unwrap();
                released.recv_timeout(WAIT).unwrap();
                events.send("earlier start").unwrap();
                Ok(())
            })
        })
    };
    starting.recv_timeout(WAIT).unwrap();
    let stop = {
        let device = device.clone();
        let events = events.clone();
        std::thread::spawn(move || {
            device.stop(|| {
                events.send("stop").unwrap();
                Ok(())
            })
        })
    };
    // The stop has asked, and waits behind the earlier start.
    let deadline = std::time::Instant::now() + WAIT;
    while !device.is_suspended() {
        assert!(std::time::Instant::now() < deadline, "the stop never asked");
        std::thread::yield_now();
    }
    device.resume();
    let reconnect = {
        let device = device.clone();
        std::thread::spawn(move || {
            device.start("test", || {
                events.send("reconnect").unwrap();
                Ok(())
            })
        })
    };
    release.send(()).unwrap();
    assert_eq!(observed.recv_timeout(WAIT).unwrap(), "earlier start");
    assert_eq!(observed.recv_timeout(WAIT).unwrap(), "stop");
    assert_eq!(observed.recv_timeout(WAIT).unwrap(), "reconnect");
    earlier.join().unwrap().unwrap();
    stop.join().unwrap().unwrap();
    reconnect.join().unwrap().unwrap();
    assert!(!device.is_suspended(), "the reconnect came after the stop");
}

#[test]
fn adding_a_stopped_container_back_clears_its_stop() {
    let target = ConnectTarget::Cloud {
        provider: CloudProvider::Container,
        account: "docker".into(),
        id: "herdr-test-adopt-after-stop".into(),
        machine: "herdr-test-adopt-after-stop".into(),
        session: "default".into(),
    };
    stop(Engine::Docker, "herdr-test-adopt-after-stop", || Ok(())).unwrap();
    assert!(suspended(&target));
    resume_device(Engine::Docker, "herdr-test-adopt-after-stop");
    assert!(!suspended(&target));
    start(Engine::Docker, "herdr-test-adopt-after-stop", || Ok(())).unwrap();
}
