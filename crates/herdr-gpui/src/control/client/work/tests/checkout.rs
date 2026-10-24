use super::*;
use std::{fs, path::Path, process::Command};

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git {args:?}");
}

#[test]
fn the_current_checkout_is_named_as_the_daemon_names_it() {
    let temp = tempfile::tempdir().unwrap();
    let main = temp.path().join("main");
    fs::create_dir_all(main.join("src")).unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    fs::write(main.join("src/a.txt"), "a\n").unwrap();
    git(&main, &["add", "-A"]);
    git(&main, &["commit", "-q", "-m", "base"]);
    let linked = temp.path().join("linked");
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            linked.to_str().unwrap(),
        ],
    );
    let common = fs::canonicalize(main.join(".git")).unwrap();
    // From a subdirectory, and from a linked worktree: one repository.
    let checkout = here::checkout(&main.join("src")).unwrap();
    assert_eq!(checkout.repo_key, common.to_str().unwrap());
    assert_eq!(checkout.branch, "main");
    assert_eq!(checkout.host, crate::usage::Host::Local);
    let checkout = here::checkout(&linked).unwrap();
    assert_eq!(checkout.repo_key, common.to_str().unwrap());
    assert_eq!(checkout.branch, "feature");
    assert_eq!(
        here::root(&main.join("src")).unwrap(),
        fs::canonicalize(&main).unwrap()
    );
    git(&linked, &["checkout", "-q", "--detach"]);
    assert!(matches!(
        here::checkout(&linked),
        Err(crate::Error::DetachedHead)
    ));
    let outside = tempfile::tempdir().unwrap();
    assert!(matches!(
        here::checkout(outside.path()),
        Err(crate::Error::NotInCheckout)
    ));
    assert!(matches!(
        rejected(crate::Error::DetachedHead),
        crate::Error::Control(crate::control::Error::Rejected {
            code: ErrorCode::InvalidRequest,
            ..
        })
    ));
    assert!(matches!(
        rejected(herdr_vcs::Error::from(crate::checkpoint::Error::Missing)),
        crate::Error::Control(crate::control::Error::Rejected {
            code: ErrorCode::NotFound,
            ..
        })
    ));
}
