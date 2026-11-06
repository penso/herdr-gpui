use super::*;
use std::path::Path;

fn run_git(directory: &Path, args: &[&str]) {
    let result = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn input_at(directory: &Path, branch: &str) -> Input {
    Input {
        repo_key: Some(directory.join(".git").to_str().unwrap().to_owned()),
        branch: branch.into(),
        checkout: Some(directory.to_str().unwrap().into()),
    }
}

fn read(input: &Input) -> Option<Ahead> {
    crate::git::ahead::read(input, Duration::from_secs(15), &|| false).unwrap()
}

/// A repository on `main` whose `origin` names `main` as its default branch.
fn repository() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    run_git(path, &["init", "-b", "main"]);
    run_git(path, &["commit", "--allow-empty", "-m", "initial"]);
    run_git(path, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    run_git(
        path,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    directory
}

#[test]
fn counts_commits_past_the_default_branch_and_names_the_tip() {
    let directory = repository();
    let path = directory.path();
    run_git(path, &["switch", "-c", "feature"]);
    let input = input_at(path, "feature");
    assert_eq!(read(&input), None, "a new branch holds nothing more");
    run_git(path, &["commit", "--allow-empty", "-m", "one"]);
    run_git(path, &["commit", "--allow-empty", "-m", "two"]);
    let ahead = read(&input).unwrap();
    assert_eq!(ahead.commits, 2);
    let tip = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(ahead.head, String::from_utf8_lossy(&tip.stdout).trim());
}

#[test]
fn pushing_leaves_the_commits_counted() {
    let directory = repository();
    let path = directory.path();
    run_git(path, &["switch", "-c", "feature"]);
    run_git(path, &["commit", "--allow-empty", "-m", "one"]);
    run_git(path, &["update-ref", "refs/remotes/origin/feature", "HEAD"]);
    assert_eq!(read(&input_at(path, "feature")).unwrap().commits, 1);
}

#[test]
fn the_default_branch_is_never_ahead_of_itself() {
    let directory = repository();
    let path = directory.path();
    run_git(path, &["commit", "--allow-empty", "-m", "local"]);
    assert_eq!(read(&input_at(path, "main")), None);
}

#[test]
fn a_repository_without_a_named_default_branch_says_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    run_git(path, &["init", "-b", "feature"]);
    run_git(path, &["commit", "--allow-empty", "-m", "initial"]);
    assert_eq!(read(&input_at(path, "feature")), None);
}

#[test]
fn a_fork_counts_against_upstream() {
    let directory = repository();
    let path = directory.path();
    run_git(path, &["switch", "-c", "feature"]);
    run_git(path, &["commit", "--allow-empty", "-m", "one"]);
    // The fork's own default branch already has that commit; upstream's lacks it.
    run_git(path, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    run_git(
        path,
        &["update-ref", "refs/remotes/upstream/main", "HEAD~1"],
    );
    run_git(
        path,
        &[
            "symbolic-ref",
            "refs/remotes/upstream/HEAD",
            "refs/remotes/upstream/main",
        ],
    );
    assert_eq!(read(&input_at(path, "feature")).unwrap().commits, 1);
}

#[test]
fn the_focused_checkout_is_asked_after_listed_work_and_kept_when_unlisted() {
    let mut peer = Peer::new();
    peer.git.set_read_ahead(true);
    let now = Instant::now();
    let answer = Ahead {
        commits: 3,
        head: "abc".into(),
    };
    peer.git.track(Some(input("feature")), true, now);
    peer.git.track_listed([], true, now);
    peer.git.poll(now);
    let (requested, job) = peer.request();
    assert!(matches!(job, Job::Status));
    peer.complete(requested, Completion::Status(Ok(status(0, 0, 0))), now);
    peer.git.poll(now);
    let (requested, job) = peer.request();
    assert!(matches!(job, Job::Ahead));
    peer.complete(requested, Completion::Ahead(Ok(Some(answer.clone()))), now);
    assert_eq!(peer.git.focused_ahead(), Some(&answer));
    assert_eq!(peer.git.ahead("/repo/.git", "feature"), Some(&answer));

    // Leaving the checkout forgets it: nothing lists it any more.
    let next = now + SCAN_INTERVAL;
    peer.git.track(None, true, next);
    peer.git.track_listed([], true, next);
    assert_eq!(peer.git.ahead("/repo/.git", "feature"), None);
}

#[test]
fn a_failed_read_clears_the_answer_and_waits_out_the_backoff() {
    let mut peer = Peer::new();
    peer.git.set_read_ahead(true);
    let now = Instant::now();
    let found = Ahead {
        commits: 1,
        head: "abc".into(),
    };
    peer.git.seed_ahead(input("other"), found, now);
    let due = now + PROBE_REFRESH;
    peer.git.track_listed([input("other")], true, due);
    peer.git.poll(due);
    let (requested, job) = peer.request();
    assert!(matches!(job, Job::Dirty));
    peer.complete(requested, Completion::Dirty(Ok(false)), due);
    peer.git.poll(due);
    let (requested, job) = peer.request();
    assert!(matches!(job, Job::Ahead));
    let failure = Error::Failed {
        operation: "test",
        details: String::new(),
    };
    peer.complete(requested, Completion::Ahead(Err(failure.into())), due);
    assert_eq!(peer.git.ahead("/repo/.git", "other"), None);

    let soon = due + PROBE_REFRESH;
    peer.git.track_listed([input("other")], true, soon);
    peer.git.poll(soon);
    let (requested, job) = peer.request();
    assert!(matches!(job, Job::Dirty), "the dirty probe keeps its pace");
    peer.complete(requested, Completion::Dirty(Ok(false)), soon);
    peer.git.poll(soon);
    assert!(peer.incoming.try_recv().is_err(), "still backing off");
}

#[test]
fn nothing_is_read_until_a_lookup_could_use_it() {
    let mut peer = Peer::new();
    let now = Instant::now();
    peer.git.track(Some(input("feature")), true, now);
    peer.git.track_listed([], true, now);
    peer.git.poll(now);
    let (requested, job) = peer.request();
    assert!(matches!(job, Job::Status));
    peer.complete(requested, Completion::Status(Ok(status(0, 0, 0))), now);
    peer.git.poll(now);
    assert!(peer.incoming.try_recv().is_err());
}

#[test]
fn a_finished_action_asks_again_at_the_next_scan() {
    let mut cache = crate::git::ahead::Cache::default();
    let now = Instant::now();
    let listed = [input("feature")];
    cache.record(input("feature"), None, now + PROBE_REFRESH);
    cache.schedule(&listed, true, now);
    assert_eq!(cache.next(), None);
    cache.expire(&input("feature"), now);
    cache.schedule(&listed, true, now);
    assert_eq!(cache.next(), Some(input("feature")));
}
