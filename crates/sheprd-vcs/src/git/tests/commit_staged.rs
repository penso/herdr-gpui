//! A commit of what is staged leaves the rest of the checkout uncommitted,
//! which is what staging one hunk from a review is for.
use super::*;

fn git_in(checkout: &str, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-C", checkout])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// A checkout with `tracked` committed, set up so `commit` needs neither
/// the user's identity nor their signing key.
fn repository() -> (tempfile::TempDir, String) {
    let directory = tempfile::tempdir().unwrap();
    let checkout = directory.path().to_str().unwrap().to_owned();
    git_in(&checkout, &["init", "-q"]);
    for (key, value) in [
        ("user.name", "Test"),
        ("user.email", "test@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        git_in(&checkout, &["config", key, value]);
    }
    std::fs::write(directory.path().join("tracked"), "original\n").unwrap();
    git_in(&checkout, &["add", "-A"]);
    git_in(&checkout, &["commit", "-qm", "initial"]);
    (directory, checkout)
}

fn run_commit(checkout: &str, message: &str) -> crate::Result<Outcome> {
    commit_staged(
        checkout,
        message,
        Instant::now() + Duration::from_secs(30),
        &|| false,
    )
}

#[test]
fn only_what_is_staged_is_committed() {
    let (directory, checkout) = repository();
    std::fs::write(directory.path().join("staged"), "new\n").unwrap();
    std::fs::write(directory.path().join("tracked"), "edited\n").unwrap();
    std::fs::write(directory.path().join("untracked"), "left\n").unwrap();
    git_in(&checkout, &["add", "staged"]);

    let outcome = run_commit(&checkout, "Only this").unwrap();

    assert!(outcome.message.contains("Only this"));
    assert_eq!(
        git_in(&checkout, &["show", "--name-only", "--format=", "HEAD"]).trim(),
        "staged"
    );
    let left = git_in(&checkout, &["status", "--porcelain"]);
    assert_eq!(left, " M tracked\n?? untracked\n");
}

#[test]
fn nothing_staged_commits_nothing() {
    let (directory, checkout) = repository();
    std::fs::write(directory.path().join("tracked"), "edited\n").unwrap();
    let error = run_commit(&checkout, "Nothing").unwrap_err();
    assert!(matches!(error, crate::Error::Git(Error::NothingStaged)));
    assert_eq!(
        git_in(&checkout, &["rev-list", "--count", "HEAD"]).trim(),
        "1"
    );
}

#[test]
fn the_status_counts_staged_files() {
    let (directory, checkout) = repository();
    std::fs::write(directory.path().join("tracked"), "edited\n").unwrap();
    std::fs::write(directory.path().join("other"), "new\n").unwrap();
    let read = || {
        checkout_status(&checkout, Instant::now() + Duration::from_secs(30), &|| {
            false
        })
        .unwrap()
    };
    assert_eq!(read().staged, 0);
    git_in(&checkout, &["add", "tracked"]);
    let status = read();
    assert_eq!(status.staged, 1);
    assert_eq!(status.untracked, 1);
}

#[test]
fn a_staged_commit_message_is_checked_like_any_other() {
    let mut git = Git::fixture(
        Input {
            checkout: Some("/work".into()),
            repo_key: None,
            branch: "main".into(),
        },
        Status::default(),
    );
    let error = git
        .start(Action::CommitStaged("  ".into()), None)
        .unwrap_err();
    assert!(matches!(error, crate::Error::Git(Error::CommitMessage)));
}
