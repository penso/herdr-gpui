use super::*;
use crate::teleport::{carry, launch::claude_project_dir};

const SESSION_HERDR: &str = r#"#!/bin/sh
printf '%s: %s\n' "$SIDE" "$*" >> "$CARRY_LOG"
case "$1 $2" in
"api snapshot") cat "$HOME/snapshot.json" ;;
"pane process-info") cat "$HOME/process.json" ;;
"workspace create")
    [ ! -e "$HOME/refuse-create" ] || exit 1
    cp "$HOME/landed.json" "$HOME/snapshot.json"
    printf '%s\n' '{"result":{"workspace":{"workspace_id":"w9"}}}' ;;
"workspace close") [ ! -e "$HOME/refuse-close" ] ;;
"pane run")
    [ ! -e "$HOME/refuse-run" ] || exit 1
    # The resumed agent becomes the foreground job, unless it never starts.
    [ -e "$HOME/refuse-start" ] || printf '%s\n' '{"result":{"process_info":{"foreground_process_group_id":50,"shell_pid":7,"foreground_processes":[{"pid":50,"argv":["/usr/bin/node","/opt/bin/claude","--resume","session-603"]}]}}}' > "$HOME/process.json" ;;
"pane close") [ ! -e "$HOME/refuse-close" ] ;;
*) echo "unexpected: $*" >&2; exit 2 ;;
esac
"#;

struct SessionFixture {
    _temp: tempfile::TempDir,
    source_home: PathBuf,
    destination_home: PathBuf,
    cwd: PathBuf,
    mapped: PathBuf,
    events: PathBuf,
    origin: Origin,
    hosts: HostRepositories,
}

fn script(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn session_fixture(relative: &str) -> SessionFixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let source_home = root.join("source home");
    let destination_home = root.join("destination home");
    let events = root.join("events");
    let make_host = |home: &Path, side: &str| {
        let bin = home.join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        script(&bin.join("herdr"), SESSION_HERDR);
        script(&bin.join("claude"), "#!/bin/sh\nexit 0\n");
        // Any accidental Git or credential command makes the test fail and
        // leaves evidence even if a caller swallows the failure.
        for program in ["git", "gh"] {
            script(
                &bin.join(program),
                "#!/bin/sh\nprintf 'forbidden\\n' >> \"$CARRY_LOG\"\nexit 99\n",
            );
        }
        let mut host = Host::new(&ConnectTarget::Local).unwrap();
        host.env = vec![
            ("HOME".into(), home.to_str().unwrap().into()),
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("SHELL".into(), "/bin/sh".into()),
            ("CARRY_LOG".into(), events.to_str().unwrap().into()),
            ("SIDE".into(), side.into()),
        ];
        host
    };
    let source_host = make_host(&source_home, "source");
    let destination_host = make_host(&destination_home, "destination");
    let cwd = source_home.join(relative);
    let mapped = destination_home.join(relative);
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir_all(&mapped).unwrap();
    let snapshot = serde_json::json!({"result": {"snapshot": {
        "workspaces": [{"workspace_id": "w1"}],
        "tabs": [{"tab_id": "w1:t1", "workspace_id": "w1"}],
        "panes": [{"pane_id": "w1:p2", "tab_id": "w1:t1", "cwd": cwd,
            "agent": "claude", "agent_session": {
                "agent": "claude", "kind": "id", "source": "herdr:claude", "value": "session-603"
            }}],
    }}});
    fs::write(source_home.join("snapshot.json"), snapshot.to_string()).unwrap();
    let process = serde_json::json!({"result": {"process_info": {
        "foreground_process_group_id": 42, "shell_pid": 7,
        "foreground_processes": [{"pid": 42, "cwd": cwd,
            "argv": ["claude", "--model", "sonnet", "--dangerously-skip-permissions", "old prompt"]}],
    }}});
    fs::write(source_home.join("process.json"), process.to_string()).unwrap();
    let landed = serde_json::json!({"result": {"snapshot": {
        "workspaces": [{"workspace_id": "w9"}],
        "tabs": [{"tab_id": "w9:t1", "workspace_id": "w9"}],
        "panes": [{"pane_id": "w9:p1", "tab_id": "w9:t1", "cwd": mapped}],
    }}});
    fs::write(destination_home.join("landed.json"), landed.to_string()).unwrap();
    let transcript = source_home.join(".claude/projects/source");
    fs::create_dir_all(&transcript).unwrap();
    fs::write(
        transcript.join("session-603.jsonl"),
        format!("{{\"cwd\":{}}}\n", serde_json::to_string(&cwd).unwrap()),
    )
    .unwrap();
    fs::write(cwd.join("private-notes.txt"), "stay here").unwrap();
    SessionFixture {
        origin: Origin {
            place: Place {
                endpoint_id: "local".into(),
                label: "Here".into(),
                host: source_host,
            },
            workspace_id: "w1".into(),
            pane_id: "w1:p2".into(),
        },
        hosts: HostRepositories {
            place: Place {
                endpoint_id: "ssh:there".into(),
                label: "There".into(),
                host: destination_host,
            },
            repositories: None,
            retired: Vec::new(),
        },
        _temp: temp,
        source_home,
        destination_home,
        cwd,
        mapped,
        events,
    }
}

fn reviewed(f: &SessionFixture) -> Review {
    carry::review(
        &f.origin,
        &f.hosts,
        &Choice::SessionOnly,
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn source_snapshot(f: &SessionFixture, edit: impl FnOnce(&mut serde_json::Value)) {
    let path = f.source_home.join("snapshot.json");
    let mut value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    edit(&mut value);
    fs::write(path, value.to_string()).unwrap();
}

#[test]
fn session_only_review_and_run_transfer_history_and_flags_without_git_or_files() {
    let f = session_fixture("notes with 'quotes'");
    let review = reviewed(&f);
    let Review::SessionOnly(session) = &review else {
        panic!("session-only review");
    };
    assert_eq!(session.cwd, f.mapped.to_str().unwrap());
    let (_, action) = session.resume().unwrap();
    assert_eq!(action, Action::Resume(AgentKind::Claude));
    let mut steps = Vec::new();
    let moved = carry::run(
        &f.origin,
        &review,
        |step| steps.push(step),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        (
            moved.endpoint_id.as_str(),
            moved.workspace_id.as_str(),
            moved.pane_id.as_str()
        ),
        ("ssh:there", "w9", "w9:p1")
    );
    assert!(moved.warnings.is_empty());
    assert!(steps.iter().all(|step| matches!(
        step,
        Step::Open | Step::Sessions | Step::Launch | Step::Retire
    )));
    let log = fs::read_to_string(&f.events).unwrap();
    assert!(!log.contains("forbidden"), "{log}");
    let launch = log.find("destination: pane run w9:p1").unwrap();
    assert!(launch < log.find("source: pane close w1:p2").unwrap());
    assert!(log.contains(&format!(
        "workspace create --cwd {} --no-focus",
        f.mapped.display()
    )));
    assert!(log.contains("--resume session-603"), "{log}");
    assert!(log.contains("--model sonnet"), "{log}");
    assert!(log.contains("--dangerously-skip-permissions"), "{log}");
    assert!(!log.contains("old prompt"));
    let copied = f
        .destination_home
        .join(".claude/projects")
        .join(claude_project_dir(f.mapped.to_str().unwrap()))
        .join("session-603.jsonl");
    let transcript = fs::read_to_string(copied).unwrap();
    assert!(transcript.contains(f.mapped.to_str().unwrap()));
    assert!(!transcript.contains(f.cwd.to_str().unwrap()));
    assert!(f.cwd.join("private-notes.txt").exists());
    assert!(!f.mapped.join("private-notes.txt").exists());
}

#[test]
fn home_itself_maps_to_destination_home() {
    let f = session_fixture("");
    let Review::SessionOnly(review) = reviewed(&f) else {
        panic!("session-only review");
    };
    assert_eq!(review.cwd, f.destination_home.to_str().unwrap());
}

#[test]
fn missing_destination_is_refused_and_never_created() {
    let f = session_fixture("notes");
    fs::remove_dir(&f.mapped).unwrap();
    let error = carry::review(
        &f.origin,
        &f.hosts,
        &Choice::SessionOnly,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        matches!(&error, Error::DestinationDirectoryMissing { path } if path == f.mapped.to_str().unwrap())
    );
    assert!(error.to_string().contains("no files are copied"));
    assert!(!f.mapped.exists());
    assert!(
        !fs::read_to_string(f.events)
            .unwrap()
            .contains("workspace create")
    );
}

#[test]
fn uncarriable_or_untrusted_sessions_are_refused() {
    for session in [
        serde_json::Value::Null,
        serde_json::json!({"agent":"grok", "kind":"id", "source":"herdr:grok", "value":"abc"}),
        serde_json::json!({"agent":"claude", "kind":"id", "source":"untrusted", "value":"abc"}),
    ] {
        let f = session_fixture("notes");
        source_snapshot(&f, |snapshot| {
            snapshot["result"]["snapshot"]["panes"][0]["agent_session"] = session
        });
        let error = carry::review(
            &f.origin,
            &f.hosts,
            &Choice::SessionOnly,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(matches!(error, Error::SessionRequired));
    }
}

#[test]
fn changed_session_or_cwd_is_refused_on_confirm() {
    let f = session_fixture("notes");
    let review = reviewed(&f);
    source_snapshot(&f, |snapshot| {
        snapshot["result"]["snapshot"]["panes"][0]["agent_session"]["value"] = "different".into()
    });
    let error = carry::run(&f.origin, &review, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(error, Error::SourceSessionChanged));
    assert!(
        !fs::read_to_string(f.events)
            .unwrap()
            .contains("workspace create")
    );
}

#[test]
fn directory_is_checked_again_on_confirm() {
    let f = session_fixture("notes");
    let review = reviewed(&f);
    fs::remove_dir(&f.mapped).unwrap();
    let error = carry::run(&f.origin, &review, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(error, Error::DestinationDirectoryMissing { .. }));
    assert!(!f.mapped.exists());
    assert!(!fs::read_to_string(f.events).unwrap().contains("pane close"));
}

#[test]
fn failed_session_transfer_leaves_source_running() {
    let f = session_fixture("notes");
    let review = reviewed(&f);
    fs::remove_file(
        f.source_home
            .join(".claude/projects/source/session-603.jsonl"),
    )
    .unwrap();
    let error = carry::run(&f.origin, &review, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(error, Error::SessionMissing { agent: "claude" }));
    // The workspace opens before the transfer, and closes again when it fails.
    let log = fs::read_to_string(f.events).unwrap();
    assert!(log.contains("destination: workspace close w9"));
    assert!(!log.contains("pane run") && !log.contains("source: pane close"));
}

#[test]
fn failed_launch_closes_only_new_workspace_and_keeps_source() {
    let f = session_fixture("notes");
    let review = reviewed(&f);
    fs::write(f.destination_home.join("refuse-run"), "").unwrap();
    let error = carry::run(&f.origin, &review, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(
        error,
        Error::Script {
            step: Step::Launch,
            ..
        }
    ));
    assert!(std::error::Error::source(&error).is_some());
    let log = fs::read_to_string(f.events).unwrap();
    assert!(log.contains("destination: workspace close w9"));
    assert!(!log.contains("source: pane close"));
}

#[test]
fn failed_cleanup_preserves_the_cause_and_workspace_identity() {
    let f = session_fixture("notes");
    let review = reviewed(&f);
    for flag in ["refuse-run", "refuse-close"] {
        fs::write(f.destination_home.join(flag), "").unwrap();
    }
    let error = carry::run(&f.origin, &review, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(
        matches!(&error, Error::SessionWorkspaceLeftBehind { workspace_id, cause } if workspace_id == "w9" && matches!(**cause, Error::Script { step: Step::Launch, .. }))
    );
    assert!(std::error::Error::source(&error).is_some());
    assert!(error.to_string().contains("w9"));
}

#[test]
fn a_plain_shell_has_no_session_only_move() {
    let f = session_fixture("notes");
    source_snapshot(&f, |snapshot| {
        let pane = &mut snapshot["result"]["snapshot"]["panes"][0];
        pane["agent_session"] = serde_json::Value::Null;
        pane["agent"] = serde_json::Value::Null;
    });
    fs::write(
        f.source_home.join("process.json"),
        r#"{"result":{"process_info":{}}}"#,
    )
    .unwrap();
    let error = carry::review(
        &f.origin,
        &f.hosts,
        &Choice::SessionOnly,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(matches!(error, Error::SessionRequired));
}

#[test]
fn an_outside_home_cwd_is_refused_before_destination_changes() {
    let f = session_fixture("notes");
    let path = f.source_home.join("process.json");
    let mut process: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let outside = f._temp.path().canonicalize().unwrap().join("outside");
    fs::create_dir_all(&outside).unwrap();
    process["result"]["process_info"]["foreground_processes"][0]["cwd"] =
        outside.to_str().unwrap().into();
    fs::write(path, process.to_string()).unwrap();
    let error = carry::review(
        &f.origin,
        &f.hosts,
        &Choice::SessionOnly,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(matches!(error, Error::OutsideHome { .. }));
    assert!(
        !fs::read_to_string(f.events)
            .unwrap()
            .contains("destination:")
    );
}

#[test]
fn a_cwd_changed_during_review_is_refused() {
    let f = session_fixture("notes");
    let review = reviewed(&f);
    let path = f.source_home.join("process.json");
    let mut process: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    process["result"]["process_info"]["foreground_processes"][0]["cwd"] =
        f.source_home.to_str().unwrap().into();
    fs::write(path, process.to_string()).unwrap();
    let error = carry::run(&f.origin, &review, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(error, Error::SourceSessionChanged));
    assert!(!fs::read_to_string(f.events).unwrap().contains("pane close"));
}

#[test]
fn a_cwd_reached_through_a_symlink_to_home_maps_home_relative() {
    let f = session_fixture("notes");
    // `/home/me` -> `/var/home/me`: the pane reports the link, `$HOME` is
    // the target. Both resolve to the same physical directory.
    let link = f._temp.path().canonicalize().unwrap().join("linked home");
    std::os::unix::fs::symlink(&f.source_home, &link).unwrap();
    let path = f.source_home.join("process.json");
    let mut process: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    process["result"]["process_info"]["foreground_processes"][0]["cwd"] =
        link.join("notes").to_str().unwrap().into();
    fs::write(path, process.to_string()).unwrap();
    let review = reviewed(&f);
    let Review::SessionOnly(review) = review else {
        panic!("expected a session-only review");
    };
    assert_eq!(review.cwd, f.mapped.to_str().unwrap());
}

#[test]
fn an_agent_that_never_starts_keeps_the_source_and_closes_the_workspace() {
    let f = session_fixture("notes");
    let review = reviewed(&f);
    // Accepted by `pane run`, but nothing takes the foreground (a failed cd).
    fs::write(f.destination_home.join("refuse-start"), "").unwrap();
    fs::write(
        f.destination_home.join("process.json"),
        r#"{"result":{"process_info":{"foreground_process_group_id":7,"shell_pid":7}}}"#,
    )
    .unwrap();
    let error = carry::run(&f.origin, &review, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(
        matches!(&error, Error::SessionNotStarted { agent: "claude", host } if host == "There"),
        "{error:?}"
    );
    let log = fs::read_to_string(f.events).unwrap();
    assert!(log.contains("destination: workspace close w9"));
    assert!(!log.contains("source: pane close"));
}

#[test]
fn a_working_agent_is_refused_at_review_and_on_confirm() {
    let f = session_fixture("notes");
    let working = |f: &SessionFixture| {
        source_snapshot(f, |snapshot| {
            snapshot["result"]["snapshot"]["panes"][0]["agent_status"] = "working".into()
        })
    };
    let review = reviewed(&f);
    working(&f);
    let error = carry::review(
        &f.origin,
        &f.hosts,
        &Choice::SessionOnly,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(matches!(error, Error::SessionAgentWorking));
    // Reviewed while idle, working by the time of Enter: nothing is created.
    let error = carry::run(&f.origin, &review, |_| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(error, Error::SessionAgentWorking));
    assert!(
        !fs::read_to_string(f.events)
            .unwrap()
            .contains("workspace create")
    );
}

#[test]
fn an_unknown_agent_status_reads_as_not_working() {
    let f = session_fixture("notes");
    source_snapshot(&f, |snapshot| {
        snapshot["result"]["snapshot"]["panes"][0]["agent_status"] = "some_future_status".into()
    });
    reviewed(&f);
}
