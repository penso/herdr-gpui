//! Comparing a branch with the branch it was cut from rather than the
//! default branch, against a real repository.
use super::*;
use std::process::Command;

/// A repository whose `feature` was cut from `rc`, itself cut from an older
/// `main`; `main` and `rc` both moved on since. Returns its directory.
fn repository() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let hooks = tempfile::tempdir().unwrap();
    let path = directory.path().to_str().unwrap().to_owned();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(["-C", &path, "-c", "user.name=Test"])
            .args(["-c", "user.email=test@example.invalid"])
            .args(["-c", "commit.gpgsign=false"])
            .arg("-c")
            .arg(format!("core.hooksPath={}", hooks.path().display()))
            .args(args)
            .env_remove("GIT_DIR")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {output:?}");
    };
    let commit = |name: &str, text: &str, message: &str| {
        std::fs::write(directory.path().join(name), text).unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", message]);
    };
    git(&["init", "-q", "-b", "main"]);
    commit("lib.rs", "one\n", "base");
    git(&["switch", "-qc", "rc"]);
    commit("rc.rs", "release fix\n", "on the release branch");
    git(&["switch", "-q", "main"]);
    commit("main.rs", "later on main\n", "main moved on");
    git(&["switch", "-q", "rc"]);
    git(&["switch", "-qc", "feature"]);
    commit("feature.rs", "the feature\n", "the feature");
    git(&["switch", "-q", "rc"]);
    commit("rc.rs", "release fix\nanother\n", "rc moved on");
    git(&["switch", "-q", "feature"]);
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

fn files(loaded: &Loaded) -> Vec<&str> {
    loaded
        .diff
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect()
}

#[test]
fn the_branch_it_was_cut_from_is_offered_first() {
    let directory = repository();
    let choices = bases(&input(&directory)).unwrap();
    let offered: Vec<(&str, Option<u64>)> = choices
        .iter()
        .map(|choice| (choice.label(), choice.behind))
        .collect();
    // The checkout's own branch is never offered. Git before 2.41 has no
    // `ahead-behind`, so it lists the names alone, in name order.
    if offered.iter().all(|(_, behind)| behind.is_none()) {
        assert_eq!(offered, [("main", None), ("rc", None)]);
        return;
    }
    assert_eq!(offered, [("rc", Some(1)), ("main", Some(2))]);
    assert_eq!(choices[0].reference, "refs/heads/rc");
}

#[test]
fn a_picked_base_is_compared_with_instead_of_the_default_branch() {
    let directory = repository();
    let input = input(&directory);
    let automatic = load(&input, Scope::Branch, None, None, false).unwrap();
    assert_eq!(automatic.base.as_deref(), Some("main"));
    assert_eq!(files(&automatic), ["feature.rs", "rc.rs"]);

    let picked = load(&input, Scope::Branch, Some("refs/heads/rc"), None, false).unwrap();
    assert_eq!(picked.base.as_deref(), Some("rc"));
    // Only the feature's own work; rc's later commit is not undone.
    assert_eq!(files(&picked), ["feature.rs"]);
    assert!(
        picked.diff.before.starts_with("rc at "),
        "{}",
        picked.diff.before
    );

    // The pick outranks the pull request's base.
    let over_hint = load(
        &input,
        Scope::Branch,
        Some("refs/heads/rc"),
        Some("main"),
        false,
    );
    assert_eq!(over_hint.unwrap().base.as_deref(), Some("rc"));
}

#[test]
fn a_missing_or_unsafe_pick_falls_back_to_the_automatic_base() {
    let directory = repository();
    let input = input(&directory);
    for picked in [
        "refs/heads/gone",
        "rc",
        "refs/tags/rc",
        "--output=/tmp/x",
        "refs/heads/a..b",
        "refs/heads/rc@{1}",
    ] {
        let loaded = load(&input, Scope::Branch, Some(picked), None, false).unwrap();
        assert_eq!(loaded.base.as_deref(), Some("main"), "{picked}");
    }
    // Uncommitted changes have no base to pick.
    let uncommitted = load(
        &input,
        Scope::Uncommitted,
        Some("refs/heads/rc"),
        None,
        false,
    );
    assert_eq!(uncommitted.unwrap().base, None);
}

#[test]
fn listed_refs_leave_out_the_branch_its_remote_copies_and_remote_heads() {
    let output = "\
refs/heads/feature 0 0
refs/heads/main 3 9
refs/heads/release/1.2 0 4
refs/remotes/origin/HEAD 3 9
refs/remotes/origin/feature 0 1
refs/remotes/origin/main 3 9
refs/remotes/fork/topic 2 4
refs/heads/odd..name 0 1
";
    let choices = bases::parse_bases(output, "feature");
    let labels: Vec<&str> = choices.iter().map(BaseChoice::label).collect();
    // Closest first, ties by name.
    assert_eq!(labels, ["fork/topic", "release/1.2", "main", "origin/main"]);
    // Without counts, from an older Git, the names alone in order.
    let plain = bases::parse_bases("refs/heads/b\nrefs/heads/a\n", "feature");
    assert_eq!(
        plain,
        [
            BaseChoice {
                reference: "refs/heads/a".into(),
                behind: None,
            },
            BaseChoice {
                reference: "refs/heads/b".into(),
                behind: None,
            },
        ]
    );
}

#[test]
fn listed_refs_are_bounded() {
    let output: String = (0..bases::MAX_BASES + 50)
        .map(|index| format!("refs/heads/b{index} 0 {index}\n"))
        .collect();
    let choices = bases::parse_bases(&output, "feature");
    assert_eq!(choices.len(), bases::MAX_BASES);
    assert_eq!(choices[0].label(), "b0");
}
