//! Settings' Restart: one restart at a time, none while off, and a
//! failure shown like any service change's.

use super::*;

static RESTARTS: AtomicUsize = AtomicUsize::new(0);

#[gpui::test]
fn a_restart_runs_once_and_reports_busy_meanwhile(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Supervisor::fixture(cx, on(), silent_daemon);
        cx.global_mut::<Supervisor>().restart = |_| {
            RESTARTS.fetch_add(1, Ordering::SeqCst);
            Ok(Outcome::Started)
        };
    });
    cx.update(|cx| {
        assert!(Supervisor::restart(cx));
        assert!(Supervisor::busy(cx));
        assert!(!Supervisor::restart(cx), "one at a time");
    });
    cx.run_until_parked();
    assert_eq!(RESTARTS.load(Ordering::SeqCst), 1);
    cx.update(|cx| {
        assert!(!Supervisor::busy(cx));
        assert!(Supervisor::failure(cx).is_none());
    });
}

#[gpui::test]
fn nothing_restarts_while_off(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Supervisor::fixture(cx, RemoteConfig::default(), silent_daemon);
        cx.global_mut::<Supervisor>().restart = |_| panic!("restarted while off");
        assert!(!Supervisor::restart(cx));
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_failed_restart_shows_its_cause(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Supervisor::fixture(cx, on(), silent_daemon);
        cx.global_mut::<Supervisor>().restart =
            |_| Err(crate::remote::Error::ProgramMissing.into());
        assert!(Supervisor::restart(cx));
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let failure = Supervisor::failure(cx).unwrap();
        assert!(matches!(
            *failure,
            crate::Error::RemoteControl(crate::remote::Error::ProgramMissing)
        ));
    });
}
