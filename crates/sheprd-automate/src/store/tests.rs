use super::*;
use sheprd_automation::report::Reason;

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("state.sqlite")).unwrap();
    (dir, store)
}

fn new_run(path: &str, scheduled: Option<i64>) -> NewRun {
    NewRun {
        path: path.to_owned(),
        name: "nightly".to_owned(),
        trigger: 0,
        trigger_name: None,
        reason: Reason::Schedule,
        scheduled,
        hash: "abc".to_owned(),
        started_at: 100,
    }
}

#[test]
fn a_firing_runs_at_most_once() {
    let (_dir, store) = store();
    let first = store.start_run(&new_run("/a.luau", Some(3_600))).unwrap();
    assert!(first.is_some());
    assert_eq!(
        store.start_run(&new_run("/a.luau", Some(3_600))).unwrap(),
        None
    );
    // Manual runs have no firing to dedupe.
    assert!(
        store
            .start_run(&new_run("/a.luau", None))
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .start_run(&new_run("/a.luau", None))
            .unwrap()
            .is_some()
    );
}

#[test]
fn runs_left_running_are_interrupted_not_resumed() {
    let (_dir, store) = store();
    let run = store
        .start_run(&new_run("/a.luau", Some(1)))
        .unwrap()
        .unwrap();
    store.start_step(run, 1, "sh", "sh `x`", 100).unwrap();
    assert_eq!(store.interrupt_running(200).unwrap(), 1);
    let row = store.run(run).unwrap().unwrap();
    assert_eq!(row.state, RunState::Interrupted);
    assert_eq!(row.finished_at, Some(200));
    let steps = store.steps(run).unwrap();
    assert_eq!(
        steps.first().map(|step| step.state),
        Some(sheprd_automation::report::StepState::Halted)
    );
    assert_eq!(store.running("/a.luau").unwrap(), None);
}

#[test]
fn requests_are_taken_once() {
    let (_dir, store) = store();
    store
        .push_request(
            &RequestKind::RunNow {
                path: "/a.luau".to_owned(),
                trigger: Some("deploy".to_owned()),
            },
            10,
        )
        .unwrap();
    store
        .push_request(&RequestKind::Stop { run: 7 }, 11)
        .unwrap();
    store.push_request(&RequestKind::Shutdown, 12).unwrap();
    let taken = store.take_requests(20).unwrap();
    let kinds = taken
        .into_iter()
        .map(|request| request.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            RequestKind::RunNow {
                path: "/a.luau".to_owned(),
                trigger: Some("deploy".to_owned()),
            },
            RequestKind::Stop { run: 7 },
            RequestKind::Shutdown,
        ]
    );
    assert!(store.take_requests(21).unwrap().is_empty());
}

#[test]
fn switches_approvals_and_cursors_round_trip() {
    let (_dir, store) = store();
    assert_eq!(store.switch("/a.luau").unwrap(), Switch::default());
    let paused = Switch {
        paused: true,
        disabled: false,
        reason: Some("paused by you".to_owned()),
    };
    store.set_switch("/a.luau", &paused).unwrap();
    assert_eq!(store.switch("/a.luau").unwrap(), paused);
    assert_eq!(store.approved_hash("/a.luau").unwrap(), None);
    store.approve("/a.luau", "h1", "terminal", 5).unwrap();
    store.approve("/a.luau", "h2", "herdr-ui", 6).unwrap();
    assert_eq!(
        store.approved_hash("/a.luau").unwrap().as_deref(),
        Some("h2")
    );
    store.set_cursor("/a.luau", 0, 100, None).unwrap();
    store.set_cursor("/a.luau", 0, 200, Some(150)).unwrap();
    store.set_cursor("/a.luau", 0, 300, None).unwrap();
    assert_eq!(store.cursor("/a.luau", 0).unwrap(), Some((300, Some(150))));
    store.clear_cursors("/a.luau").unwrap();
    assert_eq!(store.cursor("/a.luau", 0).unwrap(), None);
}

#[test]
fn tokens_add_up_per_script_and_stay_unknown_without_samples() {
    let (_dir, store) = store();
    let agent = |run: i64, path: &str, started_at: i64| AgentRow {
        run,
        position: 1,
        path: path.to_owned(),
        harness: "claude".to_owned(),
        pane: "p".to_owned(),
        cwd: "/w".to_owned(),
        started_at,
        ..AgentRow::default()
    };
    const DAY: i64 = 0;
    store.add_agent(&agent(1, "/a.luau", 1_000)).unwrap();
    assert_eq!(store.tokens_on(Some("/a.luau"), DAY).unwrap(), None);
    let measured = AgentRow {
        tokens: Some(Tokens {
            input: 10,
            output: 20,
            cache_read: 30,
            cache_write: 40,
        }),
        sampled_at: Some(1_100),
        ..agent(1, "/a.luau", 1_000)
    };
    store.sample_agent(&measured, DAY).unwrap();
    store.add_agent(&agent(2, "/b.luau", 1_000)).unwrap();
    store
        .sample_agent(
            &AgentRow {
                tokens: Some(Tokens {
                    input: 1,
                    ..Tokens::default()
                }),
                ..agent(2, "/b.luau", 1_000)
            },
            DAY,
        )
        .unwrap();
    assert_eq!(
        store
            .tokens_on(Some("/a.luau"), DAY)
            .unwrap()
            .map(Tokens::total),
        Some(100)
    );
    assert_eq!(
        store.tokens_on(None, DAY).unwrap().map(Tokens::total),
        Some(101)
    );
    assert_eq!(store.tokens_on(None, 86_400).unwrap(), None);
    assert_eq!(store.agents(true).unwrap().len(), 2);
    store
        .update_agent(&AgentRow {
            done_at: Some(1_200),
            ..measured
        })
        .unwrap();
    assert_eq!(store.agents(true).unwrap().len(), 1);
}

#[test]
fn a_day_counts_what_an_agent_started_the_day_before_spends_in_it() {
    let (_dir, store) = store();
    let (yesterday, today) = (0, 86_400);
    let at = |input: u64| AgentRow {
        run: 1,
        position: 1,
        path: "/a.luau".to_owned(),
        harness: "claude".to_owned(),
        pane: "p".to_owned(),
        cwd: "/w".to_owned(),
        started_at: today - 60,
        tokens: Some(Tokens {
            input,
            ..Tokens::default()
        }),
        ..AgentRow::default()
    };
    store.add_agent(&at(0)).unwrap();
    store.sample_agent(&at(2_000), yesterday).unwrap();
    store.sample_agent(&at(2_500), today).unwrap();
    store.sample_agent(&at(3_000), today).unwrap();
    let on = |day| store.tokens_on(None, day).unwrap().map(Tokens::total);
    assert_eq!(
        on(yesterday),
        Some(3_000),
        "its whole use, counted from yesterday"
    );
    assert_eq!(on(today), Some(1_000), "only what it spent today");
}

#[test]
fn failures_in_a_row_stop_at_a_success() {
    let (_dir, store) = store();
    let states = [
        RunState::Completed,
        RunState::Failed,
        RunState::Halted,
        RunState::Stopped,
    ];
    for (index, state) in states.into_iter().enumerate() {
        let run = store
            .start_run(&new_run("/a.luau", Some(i64::try_from(index).unwrap())))
            .unwrap()
            .unwrap();
        store.finish_run(run, state, None, 10).unwrap();
    }
    assert_eq!(store.failures_in_a_row("/a.luau").unwrap(), 3);
    let run = store
        .start_run(&new_run("/a.luau", Some(99)))
        .unwrap()
        .unwrap();
    store
        .finish_run(run, RunState::Completed, None, 11)
        .unwrap();
    assert_eq!(store.failures_in_a_row("/a.luau").unwrap(), 0);
}

#[test]
fn a_newer_schema_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    drop(Store::open(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    connection
        .execute("UPDATE meta SET value = '2' WHERE key = 'schema'", [])
        .unwrap();
    drop(connection);
    assert!(matches!(
        Store::open(&path),
        Err(crate::Error::Schema { found: 2 })
    ));
}

#[cfg(unix)]
#[test]
fn state_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
    let database = state.join("state.sqlite");
    drop(Store::open(&database).unwrap());
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&state), 0o700, "an existing directory is tightened");
    assert_eq!(mode(&database), 0o600);
}
