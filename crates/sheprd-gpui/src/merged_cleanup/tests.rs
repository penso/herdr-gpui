#![allow(clippy::unwrap_used)]
use super::*;

const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";

fn snapshot() -> ClientShellSnapshot {
    let mut snapshot = crate::sidebar::layout_tests::snapshot(7);
    for workspace in &mut snapshot.workspaces {
        workspace.agent_status = AgentStatus::Idle;
    }
    snapshot
}

fn merged(state: State) -> PullRequest {
    let mut pr = crate::pull_request::fixture().unwrap();
    pr.state = state;
    pr
}

fn candidate(snapshot: &ClientShellSnapshot, index: usize) -> Candidate {
    Candidate {
        workspace: snapshot.workspaces[index].workspace_id.clone(),
        input: workspace_input(&snapshot.workspaces[index], &Origin::Local).unwrap(),
        number: 8,
        head: HEAD.into(),
    }
}

fn clean() -> Report {
    Report {
        head: HEAD.into(),
        ..Report::default()
    }
}

#[test]
fn only_linked_worktrees_with_a_merged_pull_request_are_candidates() {
    let snapshot = snapshot();
    let pr = merged(State::Merged);
    let found: Vec<_> = candidates(&snapshot, |_| Some(&pr)).collect();
    // Workspace 3 is the main checkout; 4 and 5 are linked worktrees.
    assert_eq!(found, [candidate(&snapshot, 4), candidate(&snapshot, 5)]);
    for state in [State::Open, State::Closed, State::Unknown] {
        let pr = merged(state);
        assert_eq!(candidates(&snapshot, |_| Some(&pr)).count(), 0, "{state:?}");
    }
    assert_eq!(candidates(&snapshot, |_| None::<&PullRequest>).count(), 0);
}

#[test]
fn a_clean_checkout_at_the_merged_head_is_removed_and_the_focused_one_asks() {
    let snapshot = snapshot();
    let candidate = candidate(&snapshot, 4);
    let workspace = &snapshot.workspaces[4];
    assert_eq!(
        decide(&candidate, &clean(), workspace, false),
        Ok(Action::Remove)
    );
    assert_eq!(
        decide(&candidate, &clean(), workspace, true),
        Ok(Action::Ask)
    );
    let upper = Report {
        head: HEAD.to_ascii_uppercase(),
        ..Report::default()
    };
    assert_eq!(
        decide(&candidate, &upper, workspace, false),
        Ok(Action::Remove)
    );
}

#[test]
fn uncommitted_files_keep_the_checkout_even_when_focused() {
    let snapshot = snapshot();
    let candidate = candidate(&snapshot, 4);
    let dirty = Report {
        dirty: true,
        ..clean()
    };
    for focused in [false, true] {
        assert_eq!(
            decide(&candidate, &dirty, &snapshot.workspaces[4], focused),
            Err(Skip::Dirty)
        );
    }
}

#[test]
fn a_checkout_past_the_merged_head_or_without_one_is_kept() {
    let snapshot = snapshot();
    let workspace = &snapshot.workspaces[4];
    let mut candidate = candidate(&snapshot, 4);
    let moved = Report {
        head: "fedcba9876543210fedcba9876543210fedcba98".into(),
        ..Report::default()
    };
    assert_eq!(
        decide(&candidate, &moved, workspace, false),
        Err(Skip::HeadMoved)
    );
    // GitHub named no head commit, so nothing proves the merge covers HEAD.
    candidate.head.clear();
    let empty = Report::default();
    assert_eq!(
        decide(&candidate, &empty, workspace, false),
        Err(Skip::HeadMoved)
    );
}

#[test]
fn busy_agents_and_archive_scripts_keep_the_checkout() {
    let mut snapshot = snapshot();
    let candidate = candidate(&snapshot, 4);
    for status in [AgentStatus::Working, AgentStatus::Blocked] {
        snapshot.workspaces[4].agent_status = status;
        assert_eq!(
            decide(&candidate, &clean(), &snapshot.workspaces[4], false),
            Err(Skip::AgentBusy)
        );
    }
    snapshot.workspaces[4].agent_status = AgentStatus::Done;
    let archive = Report {
        archive: true,
        ..clean()
    };
    let workspace = &snapshot.workspaces[4];
    assert_eq!(
        decide(&candidate, &archive, workspace, false),
        Err(Skip::ArchiveScript)
    );
    // The dialog offers the archive script itself.
    assert_eq!(
        decide(&candidate, &archive, workspace, true),
        Ok(Action::Ask)
    );
}

#[test]
fn the_ledger_retries_after_its_time_and_forgets_nothing_settled_for_good() {
    let snapshot = snapshot();
    let now = Instant::now();
    let mut ledger = Ledger::default();
    let retried = candidate(&snapshot, 4);
    let settled = candidate(&snapshot, 5);
    assert!(!ledger.waiting(&retried, now));
    ledger.settle(retried.clone(), Some(now + RETRY));
    ledger.settle(settled.clone(), None);
    assert!(ledger.waiting(&retried, now));
    assert!(!ledger.waiting(&retried, now + RETRY));
    assert!(ledger.waiting(&settled, now + RETRY * 100));
    // A new head on the same branch is a new merge to consider.
    let reused = Candidate {
        head: "fedcba9876543210fedcba9876543210fedcba98".into(),
        ..settled
    };
    assert!(!ledger.waiting(&reused, now));
}

#[test]
fn the_ledger_is_bounded() {
    let snapshot = snapshot();
    let mut ledger = Ledger::default();
    for number in 0..(LEDGER_LIMIT as u64 + 10) {
        ledger.settle(
            Candidate {
                number,
                ..candidate(&snapshot, 4)
            },
            None,
        );
    }
    assert_eq!(ledger.entries.len(), LEDGER_LIMIT);
    assert_eq!(ledger.entries[0].0.number, 10);
}

#[test]
fn the_probe_reads_a_linked_checkouts_head_status_and_archive_script() {
    let directory = tempfile::tempdir().unwrap();
    let hooks = tempfile::tempdir().unwrap();
    let main = directory.path().join("main");
    let linked = directory.path().join("linked");
    std::fs::create_dir(&main).unwrap();
    let git = |dir: &std::path::Path, args: &[&str]| {
        let result = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
            ])
            .args(["-c", "commit.gpgsign=false", "-c"])
            .arg(format!("core.hooksPath={}", hooks.path().display()))
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap().trim().to_owned()
    };
    git(&main, &["init", "-b", "main"]);
    std::fs::write(main.join("tracked"), "original\n").unwrap();
    git(&main, &["add", "tracked"]);
    git(&main, &["commit", "-m", "initial"]);
    git(
        &main,
        &["worktree", "add", "-b", "feature", linked.to_str().unwrap()],
    );
    // The daemon's metadata names the repository, never the checkout.
    let input = Input {
        checkout: None,
        repo_key: Some(main.join(".git").to_str().unwrap().to_owned()),
        branch: "feature".into(),
    };
    let check = || probe(&input, &AtomicBool::new(false)).unwrap();
    let head = git(&linked, &["rev-parse", "HEAD"]);
    assert_eq!(
        check(),
        Report {
            head: head.clone(),
            ..Report::default()
        }
    );
    std::fs::write(linked.join("untracked"), "new\n").unwrap();
    assert!(check().dirty);
    std::fs::remove_file(linked.join("untracked")).unwrap();
    std::fs::write(linked.join("tracked"), "modified\n").unwrap();
    assert!(check().dirty);
    git(&linked, &["checkout", "--", "tracked"]);
    std::fs::create_dir(linked.join(".herdr")).unwrap();
    std::fs::write(
        linked.join(".herdr/worktree.toml"),
        "[scripts]\narchive = \"echo bye\"\n",
    )
    .unwrap();
    git(&linked, &["add", ".herdr"]);
    git(&linked, &["commit", "-m", "scripts"]);
    let report = check();
    assert!(report.archive && !report.dirty);
    assert_ne!(report.head, head);
    // A cancelled check is an error, never a clean report.
    assert!(probe(&input, &AtomicBool::new(true)).is_err());
}

mod window;
