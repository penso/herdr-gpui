use super::*;
use crate::testing::Fixture;

#[test]
fn worktrees_and_agents_go_through_the_cli() {
    let fixture = Fixture::new();
    let herdr = &fixture.herdr;
    assert!(herdr.server_running().unwrap());
    let created = herdr
        .create_worktree(Path::new("/repo"), "nightly/2026-10-22", Some("main"))
        .unwrap();
    assert_eq!(created.pane, "w1p1");
    assert!(created.path.ends_with("worktree"));
    herdr
        .start_agent("nightly-1", "claude", &created.pane)
        .unwrap();
    herdr
        .prompt(&created.pane, "Review main.", Some(Duration::from_secs(60)))
        .unwrap();
    let calls = fixture.calls();
    assert!(
        calls.contains(
            "worktree create --cwd /repo --branch nightly/2026-10-22 --no-focus --base main"
        ),
        "{calls}"
    );
    assert!(
        calls.contains("agent start nightly-1 --kind claude --pane w1p1 --timeout 90000"),
        "{calls}"
    );
    assert!(
        calls.contains("agent prompt w1p1 Review main. --wait --until done --timeout 60000"),
        "{calls}"
    );
}

#[test]
fn a_stop_request_ends_a_wait_for_the_agent() {
    let fixture = Fixture::new();
    let stop = Arc::new(AtomicBool::new(false));
    let stopper = {
        let stop = stop.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            stop.store(true, Ordering::Relaxed);
        })
    };
    let started = Instant::now();
    let waited = fixture
        .herdr
        .bounded(stop, Instant::now() + Duration::from_secs(3_600))
        .prompt("w1p1", "slow", Some(Duration::from_secs(3_600)));
    stopper.join().unwrap();
    assert!(matches!(waited, Err(Error::Stopped { .. })), "{waited:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn herdr_errors_are_typed() {
    let fixture = Fixture::new();
    let error = fixture.herdr.call(&["bogus", "thing"]).unwrap_err();
    match error {
        Error::Failed { command, message } => {
            assert_eq!(command, "bogus thing");
            assert_eq!(message, "unknown_command: not faked");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn pane_details_parse() {
    let fixture = Fixture::new();
    let info = fixture.herdr.process_info("w1p1").unwrap();
    assert_eq!(info.shell_pid, Some(1));
    assert!(!info.at_prompt());
    let pane = fixture.herdr.pane_agent("w1p1").unwrap();
    assert_eq!(pane.agent.as_deref(), Some("claude"));
    assert_eq!(pane.agent_status.as_deref(), Some("working"));
}

#[test]
fn a_missing_binary_is_not_running() {
    let herdr = Herdr::new(Some(PathBuf::from("/nonexistent/herdr")), None);
    assert!(!herdr.server_running().unwrap());
    assert!(matches!(herdr.notify("t", "b"), Err(Error::Launch { .. })));
}

#[test]
fn a_bounded_call_ends_at_its_deadline() {
    let fixture = Fixture::new();
    let started = Instant::now();
    let waited = fixture
        .herdr
        .bounded(
            Arc::new(AtomicBool::new(false)),
            Instant::now() + Duration::from_millis(300),
        )
        .prompt("w1p1", "slow", Some(Duration::from_secs(3_600)));
    assert!(matches!(waited, Err(Error::TimedOut { .. })), "{waited:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_hung_status_check_ends_with_the_step() {
    let fixture = Fixture::new();
    // The fake Herdr never answers `status` for this session.
    let hung = Herdr::new(
        fixture.herdr.program.clone().into(),
        Some("hung".to_owned()),
    );
    let started = Instant::now();
    let checked = hung
        .bounded(
            Arc::new(AtomicBool::new(false)),
            Instant::now() + Duration::from_millis(300),
        )
        .ensure_server();
    assert!(
        matches!(checked, Err(Error::TimedOut { .. })),
        "{checked:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}
