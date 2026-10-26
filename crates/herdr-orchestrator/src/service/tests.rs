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

mod extras;
mod glab_mapping;
mod syncing_count;
mod unreached;

/// A worker for a fresh repository in `dir`, with its repository found,
/// shown, and cached.
pub(super) fn opened(dir: &std::path::Path) -> (Service, Worker, Host) {
    let checkout = dir.join("app");
    std::fs::create_dir_all(&checkout).unwrap();
    for args in [
        &["init", "-q"][..],
        &[
            "remote",
            "add",
            "origin",
            "git@github.acme.com:group/app.git",
        ],
    ] {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&checkout)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }
    let (service, mut worker, _) = Service::prepare(Request {
        target: ConnectTarget::Local,
        checkout: checkout.to_string_lossy().into_owned(),
        workspace_id: "w1".into(),
        token: None,
        data_root: Some(dir.join("data")),
        timing: Timing {
            reload: Duration::from_millis(20),
            sync: Duration::from_secs(3600),
        },
    });
    worker.open().unwrap();
    assert!(worker.snapshot.repo.is_some());
    let host = Host::new(&ConnectTarget::Local).unwrap();
    (service, worker, host)
}
