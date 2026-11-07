//! Against a real repository: what a phone sees of a change, and that
//! looking at it leaves the repository as it was.
use super::*;
use sheprd_remote_control::control::changes::{MAX_DIFF_LINES, Scope};
use std::{path::Path, process::Command};

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .env_remove("GIT_DIR")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// `feature`, one commit past `trunk`, then edited, with a new file beside.
fn repository() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    let write = |name: &str, text: &str| std::fs::write(path.join(name), text).unwrap();
    git(path, &["init", "-q", "-b", "trunk"]);
    write("lib.rs", "one\ntwo\nthree\n");
    write("gone.rs", "bye\n");
    git(path, &["add", "-A"]);
    git(path, &["commit", "-qm", "base"]);
    git(path, &["switch", "-qc", "feature"]);
    write("branch.rs", "committed\n");
    git(path, &["add", "-A"]);
    git(path, &["commit", "-qm", "on the branch"]);
    write("lib.rs", "one\ntwo changed\nthree\n");
    write("new.rs", "fresh\n");
    std::fs::remove_file(path.join("gone.rs")).unwrap();
    directory
}

fn input(directory: &tempfile::TempDir) -> Input {
    let path = directory.path().canonicalize().unwrap();
    Input {
        checkout: Some(path.to_str().unwrap().to_owned()),
        repo_key: Some(path.join(".git").to_str().unwrap().to_owned()),
        branch: "feature".into(),
    }
}

fn paths(list: &FileList) -> Vec<&str> {
    list.files.iter().map(|file| file.path.as_str()).collect()
}

#[test]
fn uncommitted_work_lists_edits_deletions_and_new_files_with_their_counts() {
    let directory = repository();
    let list = files(&input(&directory), None, Scope::Uncommitted).unwrap();
    assert_eq!(paths(&list), ["gone.rs", "lib.rs", "new.rs"]);
    assert_eq!(list.total, 3);
    assert_eq!(list.base, None);
    let edited = &list.files[1];
    assert_eq!(edited.status, FileStatus::Modified);
    assert_eq!((edited.added, edited.removed), (Some(1), Some(1)));
    assert_eq!(list.files[0].status, FileStatus::Deleted);
    assert_eq!(list.files[2].status, FileStatus::Untracked);
}

#[test]
fn the_branch_is_compared_with_its_pull_requests_base() {
    let directory = repository();
    let list = files(&input(&directory), Some("trunk"), Scope::Branch).unwrap();
    assert_eq!(list.base.as_deref(), Some("trunk"));
    assert!(paths(&list).contains(&"branch.rs"));
    assert!(paths(&list).contains(&"lib.rs"));
}

#[test]
fn a_branch_with_nothing_to_compare_with_says_so() {
    let directory = repository();
    assert_eq!(
        files(&input(&directory), None, Scope::Branch),
        Err(Failure::NoBase)
    );
}

#[test]
fn a_file_opens_as_a_numbered_unified_diff() {
    let directory = repository();
    let diff = file_diff(&input(&directory), None, Scope::Uncommitted, "lib.rs").unwrap();
    assert_eq!(diff.shown, Shown::Text);
    assert!(!diff.truncated);
    assert_eq!(diff.lines[0].kind, LineKind::Hunk);
    let removed = diff.lines.iter().find(|l| l.kind == LineKind::Removed);
    assert_eq!(
        removed.map(|l| (l.text.as_str(), l.old)),
        Some(("two", Some(2)))
    );
    let added = diff.lines.iter().find(|l| l.kind == LineKind::Added);
    assert_eq!(
        added.map(|l| (l.text.as_str(), l.new)),
        Some(("two changed", Some(2)))
    );
}

#[test]
fn an_untracked_file_opens_wholly_added() {
    let directory = repository();
    let diff = file_diff(&input(&directory), None, Scope::Uncommitted, "new.rs").unwrap();
    assert!(
        diff.lines
            .iter()
            .any(|l| l.kind == LineKind::Added && l.text == "fresh")
    );
}

#[test]
fn only_a_listed_file_can_be_read() {
    let directory = repository();
    for path in [
        "../outside-secret",
        ".git/config",
        "/etc/passwd",
        "branch.rs",
    ] {
        assert_eq!(
            file_diff(&input(&directory), None, Scope::Uncommitted, path),
            Err(Failure::NoFile),
            "{path}"
        );
    }
}

#[test]
fn a_binary_file_is_a_note() {
    let directory = repository();
    std::fs::write(directory.path().join("logo.bin"), [0_u8, 1, 2, 0, 255]).unwrap();
    let diff = file_diff(&input(&directory), None, Scope::Uncommitted, "logo.bin").unwrap();
    assert_eq!(diff.shown, Shown::Binary);
    assert!(diff.lines.is_empty());
}

#[test]
fn a_long_diff_is_cut_and_says_so() {
    let directory = repository();
    let text = "line\n".repeat(MAX_DIFF_LINES * 2);
    std::fs::write(directory.path().join("long.txt"), text).unwrap();
    let diff = file_diff(&input(&directory), None, Scope::Uncommitted, "long.txt").unwrap();
    assert_eq!(diff.lines.len(), MAX_DIFF_LINES);
    assert!(diff.truncated);
    assert!(diff.valid());
}

#[test]
fn a_file_too_large_to_read_is_a_note_before_any_of_it_is_read() {
    let directory = repository();
    let text = "line\n".repeat(30_000);
    std::fs::write(directory.path().join("lib.rs"), text).unwrap();
    let source = input(&directory);
    let list = files(&source, None, Scope::Uncommitted).unwrap();
    let large = list
        .files
        .iter()
        .find(|file| file.path == "lib.rs")
        .unwrap();
    assert_eq!(large.shown, Shown::TooLarge);
    let diff = file_diff(&source, None, Scope::Uncommitted, "lib.rs").unwrap();
    assert_eq!(diff.shown, Shown::TooLarge);
    assert!(diff.lines.is_empty());
}

#[test]
fn looking_at_changes_leaves_the_repository_as_it_was() {
    let directory = repository();
    let path = directory.path();
    let state = || {
        (
            git(path, &["rev-parse", "HEAD"]),
            git(path, &["status", "--porcelain=v1", "--untracked-files=all"]),
            git(path, &["for-each-ref"]),
            git(path, &["stash", "list"]),
            git(path, &["diff", "--cached"]),
        )
    };
    let before = state();
    let source = input(&directory);
    files(&source, None, Scope::Uncommitted).unwrap();
    files(&source, Some("trunk"), Scope::Branch).unwrap();
    file_diff(&source, None, Scope::Uncommitted, "lib.rs").unwrap();
    file_diff(&source, Some("trunk"), Scope::Branch, "branch.rs").unwrap();
    assert_eq!(state(), before);
}

#[test]
fn a_question_for_another_session_or_workspace_is_refused() {
    let sources = Sources {
        boot_id: "now".into(),
        ..Sources::default()
    };
    let ask = |boot: &str| Ask {
        id: sheprd_remote_control::control::changes::AskId(1),
        boot_id: boot.into(),
        workspace_id: "w".into(),
        wanted: Wanted::Checks,
    };
    assert_eq!(sources.answer(&ask("old")), Answer::Failed(Failure::Gone));
    assert_eq!(sources.answer(&ask("now")), Answer::Failed(Failure::Gone));
}

fn failing_pull_request() -> PullRequest {
    let mut pr = sheprd_github::pull_request::fixture().unwrap();
    pr.base_ref_name = "trunk".into();
    pr
}

#[test]
fn only_a_failed_check_offers_a_log() {
    let detail = |outcome| CheckDetail {
        name: "test",
        outcome,
        job: Some(7),
        url: None,
    };
    assert_eq!(check(detail(Outcome::Failed)).job, Some(7));
    assert_eq!(check(detail(Outcome::Passed)).job, None);
    assert_eq!(check(detail(Outcome::Pending)).job, None);
}

#[test]
fn a_pull_request_gives_its_number_checks_and_base() {
    let pr = failing_pull_request();
    let checked = Checked::new(&pr, None);
    assert_eq!(checked.number, pr.number);
    assert_eq!(checked.base, "trunk");
    assert_eq!(checked.checks.len(), pr.check_list().count());
    assert!(checked.logs.is_none());
}

#[test]
fn a_log_is_read_only_for_a_job_the_pull_request_lists_as_failing() {
    let mut checked = Checked::new(&failing_pull_request(), None);
    checked.logs = Some(Logs {
        owner: "o".into(),
        repo: "r".into(),
        token: Arc::new(SecretString::from("token")),
    });
    checked.checks.iter_mut().for_each(|check| check.job = None);
    let source = Source {
        pull_request: Some(checked),
        workspace: workspace(),
    };
    // Refused before any request: no network in this test.
    assert_eq!(source.check_log(7), Err(Failure::NoLog));
}

#[test]
fn a_workspace_without_a_pull_request_has_no_checks_or_logs() {
    let source = Source {
        pull_request: None,
        workspace: workspace(),
    };
    assert_eq!(source.checks(), Checks::default());
    assert_eq!(source.check_log(7), Err(Failure::NoLog));
}

#[test]
fn a_gone_log_reads_as_none_and_other_refusals_as_unreadable() {
    assert_eq!(
        log_failure(sheprd_github::Error::Status(404)),
        Failure::NoLog
    );
    assert_eq!(
        log_failure(sheprd_github::Error::Status(410)),
        Failure::NoLog
    );
    assert_eq!(
        log_failure(sheprd_github::Error::Forbidden),
        Failure::Unreadable
    );
}

fn workspace() -> ClientShellWorkspace {
    ClientShellWorkspace {
        workspace_id: "w".into(),
        active_tab_id: String::new(),
        new_workspace_cwd: String::new(),
        number: 1,
        label: String::new(),
        custom_label: false,
        branch: None,
        git_ahead_behind: None,
        tokens: Vec::new(),
        worktree: None,
        focused: false,
        agent_status: sheprd_client::protocol::AgentStatus::Idle,
    }
}

#[test]
fn a_folder_without_a_branch_is_no_repository() {
    let source = Source {
        pull_request: None,
        workspace: workspace(),
    };
    assert_eq!(source.files(Scope::Uncommitted), Err(Failure::NoRepository));
}
