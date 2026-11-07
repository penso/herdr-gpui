use super::*;
use crate::{
    store::{AgentRow, NewRun},
    testing::{Fixture, git},
};
use sheprd_automation::report::{Reason, StepState, Tokens};

/// Records a run for `script` and runs its first trigger.
fn run_script(
    fixture: &Fixture,
    script: &std::path::Path,
    repo: &std::path::Path,
    stop: bool,
) -> (i64, Ended) {
    run_with(fixture, script, repo, stop, None)
}

/// As `run_script`, with the worktree scripts read at the run's start.
fn run_with(
    fixture: &Fixture,
    script: &std::path::Path,
    repo: &std::path::Path,
    stop: bool,
    worktree: Option<WorktreeScripts>,
) -> (i64, Ended) {
    let store = fixture.store();
    let source = Source::read(script).unwrap();
    let path = script.to_string_lossy().into_owned();
    let run = store
        .start_run(&NewRun {
            path: path.clone(),
            name: source.name.clone(),
            trigger: 0,
            trigger_name: None,
            reason: Reason::Manual,
            scheduled: None,
            hash: source.hash.clone(),
            started_at: now(),
        })
        .unwrap()
        .unwrap();
    let job = Job {
        run,
        path,
        repo: Some(repo.to_path_buf()),
        source,
        worktree,
        trigger: 0,
        date: "2026-10-22".to_owned(),
        time: "03:00".to_owned(),
        stop: Arc::new(AtomicBool::new(stop)),
    };
    let slots = Arc::default();
    let ended = execute(
        &fixture.paths,
        &fixture.settings(),
        &fixture.herdr,
        &slots,
        &job,
    )
    .unwrap();
    (run, ended)
}

#[test]
fn a_nightly_run_pulls_validates_and_starts_an_agent() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    let script = fixture.script(
        &repo,
        "nightly",
        r#"
herdr.manual(function(fire)
  local v = sh("echo validating on {date}; exit 1", { check = false })
  agent {
    harness = "claude",
    worktree = "nightly/" .. fire.date,
    prompt = "Validation said: " .. v.log_tail,
  }
  notify("done " .. fire.date)
end)
"#,
    );
    let (run, ended) = run_script(&fixture, &script, &repo, false);
    assert_eq!(ended.state, RunState::Completed, "{:?}", ended.detail);
    let store = fixture.store();
    let steps = store.steps(run).unwrap();
    let states = steps.iter().map(|step| step.state).collect::<Vec<_>>();
    assert_eq!(
        states,
        [
            StepState::Failed,
            StepState::Succeeded,
            StepState::Succeeded
        ]
    );
    assert!(
        steps[0].log_tail.contains("validating on 2026-10-22"),
        "{}",
        steps[0].log_tail
    );
    assert_eq!(steps[1].pane.as_deref(), Some("w1p1"));
    assert!(
        steps[1].summary.contains("nightly/2026-10-22"),
        "{}",
        steps[1].summary
    );
    let calls = fixture.calls();
    assert!(calls.contains("--branch nightly/2026-10-22"), "{calls}");
    // The prompt carries the failed step's output, filled in.
    assert!(
        calls.contains("Validation said: validating on 2026-10-22"),
        "{calls}"
    );
    assert!(
        calls.contains("notification show nightly --body done 2026-10-22"),
        "{calls}"
    );
    let agents = store.run_agents(run).unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].harness, "claude");
}

#[test]
fn pull_refuses_a_dirty_checkout() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    std::fs::write(repo.join("README.md"), "changed\n").unwrap();
    let script = fixture.script(
        &repo,
        "pull",
        "herdr.manual(function() pull(\"main\") notify(\"never\") end)\n",
    );
    let (run, ended) = run_script(&fixture, &script, &repo, false);
    assert_eq!(ended.state, RunState::Stopped);
    let steps = fixture.store().steps(run).unwrap();
    assert_eq!(steps.len(), 1);
    assert!(
        steps[0].log_tail.contains("uncommitted changes"),
        "{}",
        steps[0].log_tail
    );
}

#[test]
fn pull_fast_forwards_a_clean_checkout() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    // A clone with a remote it can pull from.
    let clone = fixture.root().join("clone");
    git(
        &fixture.root(),
        &[
            "clone",
            "-q",
            &repo.to_string_lossy(),
            &clone.to_string_lossy(),
        ],
    );
    std::fs::create_dir_all(clone.join(".herdr/automations")).unwrap();
    std::fs::write(repo.join("new.txt"), "new\n").unwrap();
    git(&repo, &["add", "new.txt"]);
    git(&repo, &["commit", "-q", "-m", "second"]);
    let script = fixture.script(
        &clone,
        "pull",
        "herdr.manual(function() pull(\"main\") end)\n",
    );
    let (_, ended) = run_script(&fixture, &script, &clone, false);
    assert_eq!(ended.state, RunState::Completed, "{:?}", ended.detail);
    assert!(clone.join("new.txt").exists());
}

#[test]
fn the_agents_per_run_cap_halts_the_run() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    let script = fixture.script(
        &repo,
        "many",
        "herdr.manual(function()\n  for i = 1, 3 do agent { harness = \"claude\", prompt = \"go\" } end\nend)\n",
    );
    let (run, ended) = run_script(&fixture, &script, &repo, false);
    assert_eq!(ended.state, RunState::Halted);
    assert!(ended.detail.unwrap().contains("agents-per-run"));
    assert_eq!(fixture.store().steps(run).unwrap().len(), 2);
}

#[test]
fn a_spent_token_budget_halts_and_pauses() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    let script = fixture.script(
        &repo,
        "budget",
        "herdr.manual(function() agent { harness = \"claude\", prompt = \"go\" } end)\n",
    );
    let path = script.to_string_lossy().into_owned();
    let store = fixture.store();
    let spent = AgentRow {
        run: 999,
        position: 1,
        path: path.clone(),
        harness: "claude".to_owned(),
        pane: "old".to_owned(),
        cwd: "/w".to_owned(),
        started_at: now(),
        done_at: Some(now()),
        tokens: Some(Tokens {
            input: 2_000_000,
            ..Tokens::default()
        }),
        ..AgentRow::default()
    };
    store.add_agent(&spent).unwrap();
    store.sample_agent(&spent, start_of_day()).unwrap();
    let (_, ended) = run_script(&fixture, &script, &repo, false);
    assert_eq!(ended.state, RunState::Halted);
    assert!(store.switch(&path).unwrap().paused);
}

#[test]
fn a_stop_request_halts_before_the_next_step() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    let script = fixture.script(
        &repo,
        "stop",
        "herdr.manual(function() notify(\"x\") end)\n",
    );
    let (_, ended) = run_script(&fixture, &script, &repo, true);
    assert_eq!(ended.state, RunState::Halted);
    assert!(ended.detail.unwrap().contains("stopped on request"));
}

#[test]
fn a_step_timeout_fails_the_step() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    let script = fixture.script(
        &repo,
        "slow",
        "herdr.manual(function() sh(\"sleep 30\", { timeout = \"1s\" }) end)\n",
    );
    let (run, ended) = run_script(&fixture, &script, &repo, false);
    assert_eq!(ended.state, RunState::Stopped);
    let steps = fixture.store().steps(run).unwrap();
    assert!(
        steps[0].log_tail.contains("ran past its timeout"),
        "{}",
        steps[0].log_tail
    );
}

#[test]
fn run_uses_the_worktree_scripts_read_when_the_run_started() {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    std::fs::write(
        repo.join(".herdr/worktree.toml"),
        "[scripts]\nrun = \"echo changed on disk\"\n",
    )
    .unwrap();
    let script = fixture.script(
        &repo,
        "validate",
        "herdr.manual(function() run(\"run\") end)\n",
    );
    let snapshot = WorktreeScripts {
        setup: None,
        run: Some("echo approved copy".to_owned()),
    };
    let (run, ended) = run_with(&fixture, &script, &repo, false, Some(snapshot));
    assert_eq!(ended.state, RunState::Completed, "{:?}", ended.detail);
    let steps = fixture.store().steps(run).unwrap();
    assert!(
        steps[0].log_tail.contains("approved copy"),
        "{}",
        steps[0].log_tail
    );
    let (_, ended) = run_with(&fixture, &script, &repo, false, None);
    assert_eq!(
        ended.state,
        RunState::Stopped,
        "no snapshot, no worktree script"
    );
}

#[test]
fn a_host_slot_is_reserved_until_its_agent_is_recorded_or_fails() {
    let fixture = Fixture::new();
    let mut settings = fixture.settings();
    settings.caps.max_agents = 1;
    let store = fixture.store();
    let source = Source::new(
        "t",
        "herdr.caps { agents_per_run = 2, tokens_per_day = 1_000, wall_time = \"1h\" }\n\
         herdr.manual(function() end)\n",
    );
    let plan = Plan::build(&source, Limits::default()).unwrap();
    let slots = Arc::default();
    // Two runs check the cap before either records its agent: only one
    // gets the host's last slot.
    let first = agent_cap(&store, &settings, &slots, &plan, "a", 0).unwrap();
    assert!(matches!(
        agent_cap(&store, &settings, &slots, &plan, "b", 0),
        Err(Halt::HostAgents)
    ));
    // A start that fails frees it.
    drop(first);
    let second = agent_cap(&store, &settings, &slots, &plan, "b", 0).unwrap();
    // Once recorded, the agent's row holds the slot instead.
    store
        .add_agent(&AgentRow {
            run: 1,
            position: 1,
            path: "b".to_owned(),
            harness: "claude".to_owned(),
            pane: "w1p1".to_owned(),
            cwd: "/tmp".to_owned(),
            started_at: now(),
            ..AgentRow::default()
        })
        .unwrap();
    drop(second);
    assert!(matches!(
        agent_cap(&store, &settings, &slots, &plan, "a", 0),
        Err(Halt::HostAgents)
    ));
}
