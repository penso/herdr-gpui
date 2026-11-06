use super::*;
use crate::testing::Fixture;
use std::time::Instant;

mod schedule;

/// Waits up to `limit` for `done`, polling.
fn wait_for(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
}

#[test]
fn the_daemon_runs_an_approved_script_on_request_and_shuts_down() {
    let fixture = Fixture::new();
    fixture.settings();
    let repo = fixture.repo();
    let script = fixture.script(
        &repo,
        "hello",
        "herdr.manual({ name = \"greet\" }, function(fire) notify(\"hello on \" .. fire.date) end)\n",
    );
    let store = fixture.store();
    store.add_repo(&repo.to_string_lossy(), 1).unwrap();
    let source = Source::read(&script).unwrap();
    let path = script.to_string_lossy().into_owned();
    store.approve(&path, &source.hash, "test", 1).unwrap();

    let paths = fixture.paths.clone();
    let daemon = thread::spawn(move || serve(&paths));
    assert!(wait_for(Duration::from_secs(10), || Lock::held(
        &fixture.paths
    )));
    assert!(matches!(
        Lock::acquire(&fixture.paths),
        Err(Error::AlreadyRunning { .. })
    ));

    store
        .push_request(
            &RequestKind::RunNow {
                path: path.clone(),
                trigger: Some("greet".to_owned()),
            },
            runner::now(),
        )
        .unwrap();
    let finished = wait_for(Duration::from_secs(20), || {
        store
            .runs(Some(&path), 1)
            .unwrap()
            .first()
            .is_some_and(|run| run.state == RunState::Completed)
    });
    assert!(
        finished,
        "the run did not complete; calls: {}",
        fixture.calls()
    );
    assert!(
        fixture
            .calls()
            .contains("notification show hello --body hello on ")
    );

    store
        .push_request(&RequestKind::Shutdown, runner::now())
        .unwrap();
    daemon.join().unwrap().unwrap();
    // Released on return; a child another test starts at that moment can
    // hold a copy of the descriptor until it runs its program.
    assert!(wait_for(Duration::from_secs(5), || !Lock::held(
        &fixture.paths
    )));
    assert_eq!(store.daemon().unwrap().and_then(|beat| beat.pid), None);
}

#[test]
fn an_unapproved_script_never_runs() {
    let fixture = Fixture::new();
    fixture.settings();
    let repo = fixture.repo();
    let script = fixture.script(
        &repo,
        "sneaky",
        "herdr.manual(function() notify(\"x\") end)\n",
    );
    let store = fixture.store();
    store.add_repo(&repo.to_string_lossy(), 1).unwrap();
    let path = script.to_string_lossy().into_owned();
    store.approve(&path, "not-its-hash", "test", 1).unwrap();
    let paths = fixture.paths.clone();
    let daemon = thread::spawn(move || serve(&paths));
    assert!(wait_for(Duration::from_secs(10), || store
        .script(&path)
        .unwrap()
        .is_some()));
    store
        .push_request(
            &RequestKind::RunNow {
                path: path.clone(),
                trigger: None,
            },
            runner::now(),
        )
        .unwrap();
    thread::sleep(Duration::from_secs(5));
    assert!(store.runs(Some(&path), 1).unwrap().is_empty());
    store
        .push_request(&RequestKind::Shutdown, runner::now())
        .unwrap();
    daemon.join().unwrap().unwrap();
    assert!(!fixture.calls().contains("notification"));
}

#[test]
fn manual_runs_pick_their_trigger() {
    let source = Source::new(
        "t",
        "herdr.caps { agents_per_run = 0, tokens_per_day = 0, wall_time = \"1h\" }\n\
         herdr.every(\"1h\", function() end)\n\
         herdr.manual({ name = \"deploy\" }, function() end)\n\
         herdr.manual(function() end)\n",
    );
    let plan = Plan::build(&source, sheprd_automation::Limits::default()).unwrap();
    assert_eq!(
        manual_trigger(&plan, None),
        Some(1),
        "the first manual trigger"
    );
    assert_eq!(manual_trigger(&plan, Some("deploy")), Some(1));
    assert_eq!(
        manual_trigger(&plan, Some("1")),
        Some(0),
        "by number from 1"
    );
    assert_eq!(manual_trigger(&plan, Some("9")), None);
    assert_eq!(manual_trigger(&plan, Some("nope")), None);
}
