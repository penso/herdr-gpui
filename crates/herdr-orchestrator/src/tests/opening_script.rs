//! Opening a project on an SSH host reads its repository, the agents the
//! host has, and the branches named here in one connection. The script runs
//! under a local shell, as on the host.
use super::*;
use crate::repo::{self, Extras};
use crate::test_executable;
use herdr_client::{ConnectTarget, ScriptLimits, run_script_command};
use std::{path::Path, process::Command, sync::atomic::AtomicBool, time::Duration};

const PROGRAMS: [&str; 2] = ["claude", "codex"];

/// Runs the opening script for `checkout` with `bin` first on `PATH`.
fn open(dir: &Path, checkout: &Path, bin: &Path) -> Result<(repo::RepoInfo, Extras)> {
    open_within(dir, checkout, bin, Duration::from_secs(10))
}

/// [`open`], ended once it prints nothing for `idle`.
fn open_within(
    dir: &Path,
    checkout: &Path,
    bin: &Path,
    idle: Duration,
) -> Result<(repo::RepoInfo, Extras)> {
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            &repo::opening_script(&checkout.to_string_lossy(), &PROGRAMS),
        ])
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("HOME", dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("gitconfig"));
    let limits = ScriptLimits {
        output: 1 << 20,
        idle,
    };
    let mut output = Vec::new();
    let ran = run_script_command(
        command,
        std::io::empty(),
        &mut output,
        limits,
        &AtomicBool::new(false),
    );
    let target = ConnectTarget::Ssh {
        target: "fixture".into(),
        session: "default".into(),
    };
    repo::opened(&target, ran.map(drop), &output, &PROGRAMS)
}

/// Everything the opening script prints for `checkout`, run to its end.
fn printed(dir: &Path, checkout: &Path, bin: &Path) -> Vec<u8> {
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            &repo::opening_script(&checkout.to_string_lossy(), &PROGRAMS),
        ])
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("HOME", dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("gitconfig"));
    let limits = ScriptLimits {
        output: 1 << 20,
        idle: Duration::from_secs(10),
    };
    let mut output = Vec::new();
    run_script_command(
        command,
        std::io::empty(),
        &mut output,
        limits,
        &AtomicBool::new(false),
    )
    .unwrap();
    output
}

/// A `git` that runs the real one, except that `stalled`, one of its
/// subcommands, prints nothing for a long while first.
fn stalling_git(bin: &Path, stalled: &str) {
    test_executable::write(
        bin.join("git"),
        format!(
            "#!/bin/sh\nfor arg; do [ \"$arg\" = {stalled} ] && sleep 30; done\nexec /usr/bin/git \"$@\"\n"
        ),
        0o755,
    )
    .unwrap();
}

fn git(checkout: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let checkout = dir.path().join("project with 'quotes'");
    std::fs::create_dir_all(&checkout).unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    test_executable::write(bin.join("claude"), "#!/bin/sh\n", 0o755).unwrap();
    (dir, checkout, bin)
}

#[test]
fn one_script_reads_the_repository_agents_and_branches() {
    let (dir, checkout, bin) = fixture();
    git(&checkout, &["init", "-q"]);
    git(
        &checkout,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/example/project.git",
        ],
    );
    git(&checkout, &["commit", "-q", "--allow-empty", "-m", "start"]);
    for branch in ["agent/377-icons-0123abcd", "agent/not-ours", "feature"] {
        git(&checkout, &["branch", branch]);
    }
    let (info, extras) = open(dir.path(), &checkout, &bin).unwrap();
    assert_eq!(info.remote.unwrap().source.repository, "example/project");
    assert_eq!(
        extras,
        Extras {
            programs: Some(vec!["claude".into()]),
            branches: Some(vec!["agent/377-icons-0123abcd".into()]),
        }
    );
    // The repository it reads is the one detection alone reads.
    let alone = Command::new("/bin/sh")
        .args(["-c", &repo::detection_script(&checkout.to_string_lossy())])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    let target = ConnectTarget::Ssh {
        target: "fixture".into(),
        session: "default".into(),
    };
    let (opened, _) = open(dir.path(), &checkout, &bin).unwrap();
    assert_eq!(
        repo::parse(&target, &String::from_utf8(alone.stdout).unwrap()).unwrap(),
        opened
    );
}

#[test]
fn a_beads_folder_outside_git_still_reads_its_agents() {
    let (dir, checkout, bin) = fixture();
    std::fs::create_dir(checkout.join(".beads")).unwrap();
    let (info, extras) = open(dir.path(), &checkout, &bin).unwrap();
    assert!(info.beads);
    assert_eq!(extras.programs, Some(vec!["claude".into()]));
    assert_eq!(extras.branches, None, "no Git, so no branches were read");
}

#[test]
fn a_branch_read_that_fails_leaves_the_detection_and_reads_them_alone() {
    let (dir, checkout, bin) = fixture();
    git(&checkout, &["init", "-q"]);
    // A `git` that answers detection but fails to list branches.
    test_executable::write(
        bin.join("git"),
        "#!/bin/sh\nfor arg; do [ \"$arg\" = for-each-ref ] && exit 1; done\nexec /usr/bin/git \"$@\"\n",
        0o755,
    )
    .unwrap();
    let (info, extras) = open(dir.path(), &checkout, &bin).unwrap();
    assert!(info.remote.is_none());
    assert_eq!(extras.programs, Some(vec!["claude".into()]));
    assert_eq!(extras.branches, None);
}

#[test]
fn a_folder_outside_git_and_beads_is_still_skipped() {
    let (dir, checkout, bin) = fixture();
    assert!(matches!(
        open(dir.path(), &checkout, &bin),
        Err(Error::NotARepository)
    ));
}

#[test]
fn a_step_that_stalls_after_the_repository_keeps_it() {
    let (dir, checkout, bin) = fixture();
    git(&checkout, &["init", "-q"]);
    let output = printed(dir.path(), &checkout, &bin);
    let target = ConnectTarget::Ssh {
        target: "fixture".into(),
        session: "default".into(),
    };
    // The script marks the repository read.
    let found = output
        .windows(b"\nfound\n".len())
        .position(|window| window == b"\nfound\n")
        .unwrap()
        + 1;
    // Cut where a stalled probe would have left it.
    let timeout = || Err(herdr_client::Error::ScriptTimeout);
    let (info, extras) = repo::opened(
        &target,
        timeout(),
        &output[..found + b"found\n".len()],
        &PROGRAMS,
    )
    .unwrap();
    assert!(info.remote.is_none());
    assert_eq!(extras, Extras::default(), "both are read on their own");
    // Cut before the repository was read: nothing to keep.
    let error = repo::opened(&target, timeout(), &output[..found], &PROGRAMS).unwrap_err();
    assert!(matches!(
        error,
        Error::Script {
            source: herdr_client::Error::ScriptTimeout,
            ..
        }
    ));
}

#[test]
fn a_detection_that_stalls_is_a_timeout() {
    let (dir, checkout, bin) = fixture();
    git(&checkout, &["init", "-q"]);
    stalling_git(&bin, "rev-parse");
    let error = open_within(dir.path(), &checkout, &bin, Duration::from_secs(1)).unwrap_err();
    assert!(
        matches!(
            error,
            Error::Script {
                source: herdr_client::Error::ScriptTimeout,
                ..
            }
        ),
        "{error:?}"
    );
}
