//! Opt-in: a session-only move of an agent pane outside any repository
//! between two isolated daemons, with a fake `claude` and transcript.

use super::*;
use crate::teleport::{
    carry::{self, Choice, Origin},
    launch::claude_project_dir,
};
use std::os::unix::fs::PermissionsExt;

/// Stands in for Claude Code: a first start reports a session the way
/// Herdr's integration does; every start records its arguments and stays.
const FAKE_CLAUDE: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/claude-argv.log"
if [ "${1:-}" != "--resume" ]; then
    herdr pane report-agent-session --source herdr:claude --agent claude \
        --agent-session-id fake-603 "$HERDR_PANE_ID"
    herdr pane report-agent --source herdr:claude --agent claude --state idle "$HERDR_PANE_ID"
fi
while :; do sleep 1; done
"#;

fn pane_of(daemon: &Daemon, workspace: &str) -> String {
    let snapshot = daemon.snapshot();
    let tab = snapshot.tabs_of(workspace)[0].tab_id.clone();
    snapshot
        .panes
        .iter()
        .find(|pane| pane.tab_id == tab)
        .unwrap()
        .pane_id
        .clone()
}

#[test]
#[ignore = "requires HERDR_TEST_BINARY and starts two isolated daemons"]
fn session_only_move_between_two_daemons() {
    let binary =
        PathBuf::from(std::env::var_os("HERDR_TEST_BINARY").expect("set HERDR_TEST_BINARY"));
    assert!(binary.is_absolute() && binary.is_file());
    // Daemons put the binary's directory on PATH; give it its own directory
    // holding only `herdr` and the fake `claude`, so no real agent runs.
    let tools = tempfile::tempdir().unwrap();
    let herdr = tools.path().join("herdr");
    fs::copy(&binary, &herdr).unwrap();
    let claude = tools.path().join("claude");
    fs::write(&claude, FAKE_CLAUDE).unwrap();
    fs::set_permissions(&claude, fs::Permissions::from_mode(0o755)).unwrap();
    let a = Daemon::start(&herdr, "a");
    let b = Daemon::start(&herdr, "b");

    // A non-repository directory under each fake home.
    let notes_a = a.dir.join("notes");
    let notes_b = b.dir.join("notes");
    fs::create_dir(&notes_a).unwrap();
    let created = a.herdr(&[
        "workspace",
        "create",
        "--cwd",
        notes_a.to_str().unwrap(),
        "--no-focus",
    ]);
    let workspace = created["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let pane = pane_of(&a, &workspace);
    a.host
        .herdr_ok(
            Step::Review,
            &[
                "pane",
                "run",
                &pane,
                "claude --model sonnet 'initial prompt'",
            ],
            &AtomicBool::new(false),
        )
        .unwrap();
    wait_for("the fake agent's session", || {
        a.snapshot()
            .pane(&pane)
            .is_some_and(|pane| pane.agent_session.is_some())
    });
    // The transcript Claude would have written, naming the source cwd.
    let project = a
        .dir
        .join(".claude/projects")
        .join(claude_project_dir(notes_a.to_str().unwrap()));
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join("fake-603.jsonl"),
        format!("{{\"cwd\":\"{}\",\"text\":\"hello\"}}\n", notes_a.display()),
    )
    .unwrap();

    let origin = Origin {
        place: Place {
            endpoint_id: "a".into(),
            label: "A".into(),
            host: a.host.clone(),
        },
        workspace_id: workspace.clone(),
        pane_id: pane.clone(),
    };
    let destination = HostRepositories {
        place: Place {
            endpoint_id: "b".into(),
            label: "B".into(),
            host: b.host.clone(),
        },
        repositories: None,
        retired: Vec::new(),
    };
    let cancelled = AtomicBool::new(false);

    // The destination lacks ~/notes: refused, never created.
    let missing = carry::review(&origin, &destination, &Choice::SessionOnly, &cancelled);
    assert!(
        matches!(
            missing,
            Err(super::super::Error::DestinationDirectoryMissing { .. })
        ),
        "{missing:?}"
    );
    assert!(!notes_b.exists());

    fs::create_dir(&notes_b).unwrap();
    let review = carry::review(&origin, &destination, &Choice::SessionOnly, &cancelled).unwrap();
    let carried = carry::run(&origin, &review, |_| {}, &cancelled).unwrap();

    // The transcript landed under B's home, with the cwd rewritten.
    let moved = b
        .dir
        .join(".claude/projects")
        .join(claude_project_dir(notes_b.to_str().unwrap()))
        .join("fake-603.jsonl");
    let transcript = fs::read_to_string(&moved).unwrap();
    assert!(
        transcript.contains(notes_b.to_str().unwrap()),
        "{transcript}"
    );
    assert!(
        !transcript.contains(notes_a.to_str().unwrap()),
        "{transcript}"
    );

    // The agent resumed on B in ~/notes with its kept flags, not the prompt.
    wait_for("the resumed agent", || {
        fs::read_to_string(b.dir.join("claude-argv.log"))
            .is_ok_and(|log| log.contains("--resume fake-603"))
    });
    let argv = fs::read_to_string(b.dir.join("claude-argv.log")).unwrap();
    assert!(argv.contains("--model sonnet"), "{argv}");
    assert!(!argv.contains("initial prompt"), "{argv}");
    let landed = b.snapshot();
    let landed_pane = landed.pane(&carried.pane_id).unwrap();
    assert_eq!(
        landed_pane
            .foreground_cwd
            .as_deref()
            .or(landed_pane.cwd.as_deref()),
        notes_b.to_str()
    );

    // The source pane closed; no Git ran anywhere (no repository exists).
    wait_for("the source pane to close", || {
        a.snapshot().pane(&pane).is_none()
    });
    assert!(!notes_a.join(".git").exists() && !notes_b.join(".git").exists());
}
