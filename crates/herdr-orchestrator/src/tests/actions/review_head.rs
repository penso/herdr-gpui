//! A review's worktree starts at the pull request's head, fetched from the
//! remote when the repository lacks it, rather than at the main checkout.
use super::*;
use crate::actions::{PullHead, pull_head};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// A GitHub-like remote of the site's repository, `origin`, whose pull
/// request #7 has a head commit the repository has never fetched: only
/// `refs/pull/7/head` reaches it, as on GitHub. Returns that commit and a
/// checkout of it standing in for the worktree Herdr makes.
fn pull_request_on_remote(dir: &Path, site: &Site) -> (String, PathBuf) {
    let root = Path::new(&site.main_root);
    let remote = dir.join("remote.git");
    let clone = |to: &Path| {
        let status = Command::new("git")
            .args(["clone", "-q", "--bare"])
            .arg(root)
            .arg(to)
            .status()
            .unwrap();
        assert!(status.success());
    };
    clone(&remote);
    let work = dir.join("contributor");
    let status = Command::new("git")
        .args(["clone", "-q"])
        .arg(&remote)
        .arg(&work)
        .status()
        .unwrap();
    assert!(status.success());
    git(
        &work,
        &["commit", "-q", "--allow-empty", "-m", "the change"],
    );
    let head = git(&work, &["rev-parse", "HEAD"]);
    git(&work, &["push", "-q", "origin", "HEAD:refs/pull/7/head"]);
    git(
        root,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    (head, work)
}

fn review(head: &str, remote: &str) -> DispatchRequest {
    let mut request = request();
    request.branch = "agent/review-7-the-change-1234567-3af51766".into();
    request.pull_request = Some(PullHead {
        number: 7,
        sha: head.into(),
        remote: remote.into(),
    });
    request
}

fn has_commit(site: &Site, commit: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(&site.main_root)
        .args(["cat-file", "-e", &format!("{commit}^{{commit}}")])
        .status()
        .unwrap()
        .success()
}

#[test]
fn a_review_starts_its_worktree_at_the_pull_requests_head() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let (head, checkout) = pull_request_on_remote(dir.path(), &site);
    std::fs::write(
        dir.path().join("home/worktree"),
        checkout.to_string_lossy().as_bytes(),
    )
    .unwrap();
    let main_head = git(Path::new(&site.main_root), &["rev-parse", "HEAD"]);
    assert_ne!(head, main_head);
    assert!(!has_commit(&site, &head), "the repository lacks the head");

    perform(
        &site,
        &Action::Dispatch(Box::new(review(&head, "origin"))),
        &AtomicBool::new(false),
    )
    .unwrap();
    let calls = calls(dir.path());
    assert_eq!(
        calls[1],
        format!(
            "worktree create --workspace w1 --branch agent/review-7-the-change-1234567-3af51766 --base {head} --no-focus"
        )
    );
    assert!(has_commit(&site, &head), "the head was fetched");
    // Only FETCH_HEAD was written: no branch or remote ref for the PR.
    let refs = git(
        Path::new(&site.main_root),
        &["for-each-ref", "--format=%(refname)"],
    );
    assert!(!refs.contains("pull"), "{refs}");
    let run = Store::open(&site.database)
        .unwrap()
        .runs()
        .unwrap()
        .remove(0);
    assert_eq!(run.state, RunState::Running);
}

#[test]
fn a_head_the_repository_already_has_is_not_fetched_again() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let (head, checkout) = pull_request_on_remote(dir.path(), &site);
    git(
        Path::new(&site.main_root),
        &["fetch", "-q", "origin", "refs/pull/7/head"],
    );
    std::fs::write(
        dir.path().join("home/worktree"),
        checkout.to_string_lossy().as_bytes(),
    )
    .unwrap();
    // A remote that does not exist would fail any fetch.
    perform(
        &site,
        &Action::Dispatch(Box::new(review(&head, "nowhere"))),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(calls(dir.path())[1].contains(&format!(" --base {head} ")));
}

#[test]
fn a_pull_request_that_moved_is_refused_before_a_worktree_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    pull_request_on_remote(dir.path(), &site);
    // The head the prompt named is no longer what refs/pull/7/head holds.
    let stale = "0123456789abcdef0123456789abcdef01234567";
    let error = perform(
        &site,
        &Action::Dispatch(Box::new(review(stale, "origin"))),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        matches!(error, Error::Review(pull_head::Error::Moved { number: 7 })),
        "{error}"
    );
    assert!(
        !calls(dir.path())
            .iter()
            .any(|call| call.starts_with("worktree create")),
        "no worktree was made"
    );
    let run = Store::open(&site.database)
        .unwrap()
        .runs()
        .unwrap()
        .remove(0);
    assert_eq!(run.state, RunState::Failed);
    assert!(run.message.unwrap().contains("changed since it was read"));
}

#[test]
fn a_branch_herdr_checked_out_elsewhere_is_refused_and_its_checkout_removed() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let (head, _) = pull_request_on_remote(dir.path(), &site);
    // The branch already existed at the main checkout's commit, and Herdr
    // checked it out there rather than at --base.
    std::fs::write(dir.path().join("home/worktree"), site.main_root.as_bytes()).unwrap();
    let error = perform(
        &site,
        &Action::Dispatch(Box::new(review(&head, "origin"))),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(
        matches!(
            &error,
            Error::Review(pull_head::Error::Branch { number: 7, branch })
                if branch == "agent/review-7-the-change-1234567-3af51766"
        ),
        "{error}"
    );
    let calls = calls(dir.path());
    assert!(
        !calls.iter().any(|call| call.starts_with("agent start")),
        "no agent started on the wrong code"
    );
    assert_eq!(
        calls.last().unwrap(),
        "worktree remove --workspace w9 --force"
    );
}

#[test]
fn a_head_or_remote_git_could_misread_is_refused_before_git_runs() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let head = "a".repeat(40);
    for (pull, expected) in [
        (review("HEAD~1", "origin"), "is not a commit ID"),
        (review(&"g".repeat(40), "origin"), "is not a commit ID"),
        (
            review(&head, "--upload-pack=touch pwned"),
            "is not one Git can fetch from",
        ),
        (review(&head, ""), "is not one Git can fetch from"),
    ] {
        let error = perform(
            &site,
            &Action::Dispatch(Box::new(pull)),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(matches!(error, Error::Review(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
    }
    assert!(!Path::new(&site.main_root).join("pwned").exists());
    assert!(
        !calls(dir.path())
            .iter()
            .any(|call| call.starts_with("worktree create"))
    );
}
