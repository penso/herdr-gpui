use super::*;
use std::{path::Path, process::Command};

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "-C",
        ])
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, crate::teleport::Host) {
    let home = tempfile::tempdir().unwrap();
    let repo = home.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    let mut host = crate::teleport::Host::new(&ConnectTarget::Local).unwrap();
    host.env = vec![("HOME".into(), home.path().to_str().unwrap().into())];
    (home, repo, host)
}

fn scan(host: &crate::teleport::Host) -> Vec<Checkout> {
    let output = host.capture(&script(), &AtomicBool::new(false)).unwrap();
    parse(std::str::from_utf8(&output).unwrap())
}

#[test]
fn unpushed_commits_survive_switching_branches_and_tag_only_reachability() {
    let (_home, repo, host) = fixture();
    assert!(scan(&host).is_empty(), "an unborn branch is clean");
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    git(&repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    assert!(scan(&host).is_empty());
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    git(
        &repo,
        &["commit", "-q", "--allow-empty", "-m", "unpublished"],
    );
    let expected = vec![Checkout {
        path: "~/repo".into(),
        changed: 0,
        unpushed: 1,
    }];
    assert_eq!(scan(&host), expected);
    git(&repo, &["checkout", "-q", "main"]);
    assert_eq!(scan(&host), expected, "HEAD is pushed, feature is not");
    git(&repo, &["tag", "saved-work", "feature"]);
    git(&repo, &["branch", "-D", "feature"]);
    assert_eq!(scan(&host), expected, "local tags also hold work");
    git(&repo, &["checkout", "-q", "--detach", "saved-work"]);
    git(&repo, &["tag", "-d", "saved-work"]);
    assert_eq!(scan(&host), expected, "detached HEAD still counts");
}

#[test]
fn git_status_failure_is_an_unchecked_scan_even_with_dirty_files() {
    let (_home, repo, host) = fixture();
    std::fs::write(repo.join("untracked"), "work").unwrap();
    std::fs::write(repo.join(".git/config"), "[invalid config").unwrap();
    assert!(matches!(
        host.capture(&script(), &AtomicBool::new(false)),
        Err(sheprd_client::Error::ScriptExit { .. })
    ));
}

#[test]
fn git_history_failure_is_an_unchecked_scan_not_an_unborn_branch() {
    let (_home, repo, host) = fixture();
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    std::fs::write(
        repo.join(".git/refs/heads/broken"),
        "1111111111111111111111111111111111111111\n",
    )
    .unwrap();
    git(&repo, &["status", "--porcelain=v1"]);
    assert!(matches!(
        host.capture(&script(), &AtomicBool::new(false)),
        Err(sheprd_client::Error::ScriptExit { .. })
    ));
}
