use super::*;
use gpui::TestAppContext;
use std::sync::atomic::{AtomicUsize, Ordering};

fn status(connected: usize) -> Status {
    Status {
        version: "dev".into(),
        port: 7443,
        addresses: vec!["https://192.168.1.20:7443".into()],
        fingerprint: "5F:2A".into(),
        herdr: true,
        chats: 1,
        devices: 2,
        connected,
    }
}

fn device(id: &str) -> Device {
    Device {
        id: DeviceId(id.into()),
        name: id.into(),
        created_ms: 0,
        last_seen_ms: None,
        push: true,
        connected: false,
    }
}

fn on() -> RemoteConfig {
    RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    }
}

#[test]
fn a_poll_reports_only_what_changed() {
    let mut supervisor = Supervisor {
        config: on(),
        ..Supervisor::default()
    };
    supervisor.polling = true;
    let devices = Response::Devices {
        devices: vec![device("d1")],
    };
    assert!(supervisor.polled(Ok(Response::Status(status(1))), Some(Ok(devices.clone()))));
    assert!(!supervisor.polling);
    assert_eq!(supervisor.health, Health::Running(status(1)));
    assert_eq!(supervisor.devices.as_deref().map(<[Device]>::len), Some(1));
    // The same answer changes nothing; one without devices keeps the list.
    assert!(!supervisor.polled(Ok(Response::Status(status(1))), Some(Ok(devices))));
    assert!(!supervisor.polled(Ok(Response::Status(status(1))), None));
    assert!(supervisor.devices.is_some());
    // A daemon that stops answering forgets its devices.
    assert!(supervisor.polled(Err(control::Error::NoAnswer), None));
    let Health::NotRunning(reason) = &supervisor.health else {
        panic!("{:?}", supervisor.health);
    };
    assert!(reason.contains("without answering"));
    assert!(supervisor.devices.is_none());
    assert!(supervisor.polled(Ok(Response::Done), None));
    assert!(matches!(supervisor.health, Health::NotRunning(_)));
}

#[test]
fn a_poll_answered_after_turning_off_is_dropped() {
    let mut supervisor = Supervisor {
        polling: true,
        ..Supervisor::default()
    };
    assert!(!supervisor.polled(Ok(Response::Status(status(1))), None));
    assert_eq!(supervisor.health, Health::Off);
    assert!(!supervisor.polling);
}

#[test]
fn leases_count_their_holders() {
    let supervisor = Supervisor::default();
    let lease = || {
        supervisor.watchers.set(supervisor.watchers.get() + 1);
        Lease(supervisor.watchers.clone())
    };
    let (first, second) = (lease(), lease());
    assert_eq!(supervisor.watchers.get(), 2);
    drop(first);
    assert_eq!(supervisor.watchers.get(), 1);
    drop(second);
    assert_eq!(supervisor.watchers.get(), 0);
}

static ASKED: AtomicUsize = AtomicUsize::new(0);
static LISTED: AtomicUsize = AtomicUsize::new(0);

fn connected_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::Status => {
            ASKED.fetch_add(1, Ordering::SeqCst);
            Ok(Response::Status(status(2)))
        }
        Request::Devices => {
            LISTED.fetch_add(1, Ordering::SeqCst);
            Ok(Response::Devices {
                devices: vec![device("d1"), device("d2")],
            })
        }
        _ => Ok(Response::Done),
    }
}

#[gpui::test]
fn the_status_bar_alone_asks_for_status_and_a_lease_for_devices(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, on(), connected_daemon));
    cx.update(Supervisor::tick);
    cx.run_until_parked();
    cx.update(|cx| {
        assert_eq!(Supervisor::connected(cx), 2);
        assert!(
            Supervisor::devices(cx).is_none(),
            "nobody asked for devices"
        );
    });
    assert_eq!(ASKED.load(Ordering::SeqCst), 1);
    assert_eq!(LISTED.load(Ordering::SeqCst), 0);
    // Not due again for a while without a lease.
    cx.update(Supervisor::tick);
    cx.run_until_parked();
    assert_eq!(ASKED.load(Ordering::SeqCst), 1);
    // A lease asks at once, devices included.
    let lease = cx.update(Supervisor::lease);
    assert!(lease.is_some());
    cx.update(Supervisor::tick);
    cx.run_until_parked();
    assert_eq!(ASKED.load(Ordering::SeqCst), 2);
    assert_eq!(LISTED.load(Ordering::SeqCst), 1);
    cx.update(|cx| assert_eq!(Supervisor::devices(cx).map(<[Device]>::len), Some(2)));
}

static OFF_ASKED: AtomicUsize = AtomicUsize::new(0);

fn counting_daemon(_: &Path, _: &Request) -> Result<Response, control::Error> {
    OFF_ASKED.fetch_add(1, Ordering::SeqCst);
    Err(control::Error::NoAnswer)
}

#[gpui::test]
fn nothing_is_asked_while_remote_control_is_off(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, RemoteConfig::default(), counting_daemon));
    let _lease = cx.update(Supervisor::lease);
    cx.update(Supervisor::tick);
    cx.run_until_parked();
    assert_eq!(OFF_ASKED.load(Ordering::SeqCst), 0);
    cx.update(|cx| {
        assert_eq!(Supervisor::health(cx), Health::Off);
        assert_eq!(Supervisor::connected(cx), 0);
    });
}

static CHANGES: AtomicUsize = AtomicUsize::new(0);

fn silent_daemon(_: &Path, _: &Request) -> Result<Response, control::Error> {
    Err(control::Error::NoAnswer)
}

#[gpui::test]
fn the_service_changes_once_per_settings_change(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Supervisor::fixture(cx, on(), silent_daemon);
        cx.global_mut::<Supervisor>().reconcile = |_, _| {
            CHANGES.fetch_add(1, Ordering::SeqCst);
            Ok(Outcome::Installed)
        };
    });
    for _ in 0..3 {
        cx.update(Supervisor::tick);
        cx.run_until_parked();
    }
    assert_eq!(CHANGES.load(Ordering::SeqCst), 0, "already in line");
    cx.update(|cx| {
        Supervisor::apply_config(cx, RemoteConfig { port: 9000, ..on() });
    });
    cx.run_until_parked();
    assert_eq!(CHANGES.load(Ordering::SeqCst), 1);
    // The same settings again change nothing.
    cx.update(|cx| {
        Supervisor::apply_config(cx, RemoteConfig { port: 9000, ..on() });
    });
    cx.run_until_parked();
    assert_eq!(CHANGES.load(Ordering::SeqCst), 1);
}

#[gpui::test]
fn a_failed_change_waits_for_retry(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Supervisor::fixture(cx, on(), silent_daemon);
        cx.global_mut::<Supervisor>().reconcile =
            |_, _| Err(crate::remote::Error::ProgramMissing.into());
        Supervisor::apply_config(cx, RemoteConfig { port: 9000, ..on() });
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let failure = Supervisor::failure(cx).unwrap();
        assert!(matches!(
            *failure,
            crate::Error::RemoteControl(crate::remote::Error::ProgramMissing)
        ));
        assert!(!Supervisor::busy(cx));
        cx.global_mut::<Supervisor>().reconcile = |_, _| Ok(Outcome::Installed);
    });
    cx.update(Supervisor::tick);
    cx.run_until_parked();
    cx.update(|cx| assert!(Supervisor::failure(cx).is_some(), "not retried on its own"));
    cx.update(Supervisor::retry);
    cx.run_until_parked();
    cx.update(|cx| assert!(Supervisor::failure(cx).is_none()));
}
