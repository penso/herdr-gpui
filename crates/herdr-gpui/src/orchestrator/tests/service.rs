use super::*;
use crate::orchestrator::location::database_path_in;
use herdr_client::ConnectTarget;
use std::{
    process::Command,
    time::{Duration, Instant},
};

/// A repository whose only remote is GitLab, so no source reaches the network.
fn repository(dir: &std::path::Path) -> String {
    let checkout = dir.join("app");
    std::fs::create_dir_all(&checkout).unwrap();
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .arg("-C")
            .arg(&checkout)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["remote", "add", "origin", "git@gitlab.com:group/app.git"]);
    checkout.to_string_lossy().into_owned()
}

fn start(dir: &std::path::Path, checkout: String) -> Service {
    Service::start(Request {
        target: ConnectTarget::Local,
        checkout,
        token: None,
        data_root: Some(dir.join("data")),
        timing: Timing {
            reload: Duration::from_millis(20),
            sync: Duration::from_secs(3600),
        },
    })
    .unwrap()
}

/// Polls `service` until `ready` holds, for at most ten seconds.
fn wait(service: &Service, ready: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last = None;
    while Instant::now() < deadline {
        if let Some(snapshot) = service.poll() {
            if ready(&snapshot) {
                return snapshot;
            }
            last = Some(snapshot);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no matching snapshot; last: {last:?}");
}

#[test]
fn the_worker_shows_runs_another_application_writes() {
    let dir = tempfile::tempdir().unwrap();
    let service = start(dir.path(), repository(dir.path()));
    let snapshot = wait(&service, |snapshot| snapshot.repo.is_some());
    assert_eq!(snapshot.access, Some(Access::ReadWrite));
    assert!(matches!(
        snapshot.sources.as_slice(),
        [SourceStatus {
            key: SourceKey {
                provider: Provider::Gitlab,
                ..
            },
            state: SyncState::Unsupported
        }]
    ));

    // agent-launcher, through its own connection, dispatches a run.
    let repo = snapshot.repo.unwrap().repository;
    let path = database_path_in(&dir.path().join("data"), &repo);
    rusqlite::Connection::open(path)
        .unwrap()
        .execute_batch(
            "INSERT INTO runs (id, issue_key, agent, state_json, started_at, updated_at) VALUES \
             ('r1', 'github:github.com:g/app:1', 'claude', '\"running\"', \
             '2026-10-08T06:12:44+00:00', '2026-10-08T06:12:44+00:00')",
        )
        .unwrap();
    let snapshot = wait(&service, |snapshot| !snapshot.runs.is_empty());
    assert_eq!(snapshot.runs[0].owner, Owner::AgentLauncher);
    assert_eq!(snapshot.runs[0].state, RunState::Running);
}

#[test]
fn a_folder_outside_git_reports_why() {
    let dir = tempfile::tempdir().unwrap();
    let service = start(dir.path(), dir.path().to_string_lossy().into_owned());
    let snapshot = wait(&service, |snapshot| snapshot.error.is_some());
    assert!(matches!(
        snapshot.error.as_deref(),
        Some(Error::NotARepository)
    ));
    assert!(!dir.path().join("data").exists());
}
