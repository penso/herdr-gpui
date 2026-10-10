#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn closing_the_view_does_not_cancel_an_accepted_action() {
    let (commands, _receiver) = mpsc::sync_channel(COMMANDS);
    let cancelled = Arc::new(AtomicBool::new(false));
    let service = Service {
        commands,
        cancelled: cancelled.clone(),
        mailbox: Arc::default(),
        notices: Arc::default(),
    };
    let (started, starting) = mpsc::sync_channel(1);
    let (resume, resumed) = mpsc::sync_channel(1);
    let action = spawn_action(move |cancelled| {
        started.send(()).unwrap();
        resumed.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!cancelled.load(Ordering::Relaxed));
    })
    .unwrap();
    starting.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(service);
    assert!(cancelled.load(Ordering::Relaxed));
    resume.send(()).unwrap();
    action.join().unwrap();
}
