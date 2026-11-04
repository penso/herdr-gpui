use super::*;
use crate::repo;
use sheprd_client::ConnectTarget;
use sheprd_remote::teleport::Host;
use std::{
    error::Error as _,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    time::Duration,
};

struct Fixture {
    dir: tempfile::TempDir,
    checkout: PathBuf,
}

impl Fixture {
    fn new(git: bool, beads: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let checkout = dir.path().join("project with 'quotes'");
        std::fs::create_dir_all(&checkout).unwrap();
        if git {
            gix::init(&checkout).unwrap();
        }
        if beads {
            std::fs::create_dir(checkout.join(".beads")).unwrap();
        }
        Self { dir, checkout }
    }

    fn probe(&self, configure: impl FnOnce(&mut Command)) -> Result<repo::RepoInfo> {
        self.probe_at(&self.checkout, configure)
    }

    fn probe_at(
        &self,
        checkout: &Path,
        configure: impl FnOnce(&mut Command),
    ) -> Result<repo::RepoInfo> {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", &repo::detection_script(&checkout.to_string_lossy())])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.dir.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.dir.path().join("gitconfig"));
        configure(&mut command);
        let output =
            Host::capture_command(command, Duration::from_secs(5), &AtomicBool::new(false))
                .map_err(error::script("Reading the repository"))?;
        repo::parse(
            &ConnectTarget::Ssh {
                target: "fixture".into(),
                session: "default".into(),
            },
            &String::from_utf8(output).unwrap(),
        )
    }
}

fn assert_script_failure(error: Error, expected_status: i32) {
    assert!(error.source().is_some());
    assert!(
        error
            .to_string()
            .starts_with("Reading the repository failed:")
    );
    let Error::Script {
        source: sheprd_client::Error::ScriptExit { status, stderr },
        ..
    } = error
    else {
        panic!("expected a script failure, got {error:?}");
    };
    assert_eq!(status.code(), Some(expected_status), "{stderr}");
    assert!(!stderr.is_empty());
}

#[test]
fn ordinary_folders_are_skipped_and_standalone_beads_are_kept() {
    let plain = Fixture::new(false, false);
    assert!(matches!(plain.probe(|_| {}), Err(Error::NotARepository)));
    let beads = Fixture::new(false, true);
    let info = beads.probe(|_| {}).unwrap();
    assert!(info.beads);
    assert!(info.remote.is_none());
    assert_eq!(
        Path::new(&info.main_root),
        beads.checkout.canonicalize().unwrap()
    );
}

#[test]
fn a_git_subdirectory_keeps_its_repository_and_remote() {
    let fixture = Fixture::new(true, true);
    let subdirectory = fixture.checkout.join("src");
    std::fs::create_dir(&subdirectory).unwrap();
    std::fs::write(
        fixture.checkout.join(".git/config"),
        "[remote \"origin\"]\nurl = https://github.com/example/project.git\n",
    )
    .unwrap();
    let info = fixture.probe_at(&subdirectory, |_| {}).unwrap();
    assert!(info.beads);
    assert_eq!(
        Path::new(&info.main_root),
        fixture.checkout.canonicalize().unwrap()
    );
    assert_eq!(info.remote.unwrap().source.repository, "example/project");
}

#[test]
fn missing_git_is_a_failure_even_with_beads() {
    for beads in [false, true] {
        let fixture = Fixture::new(true, beads);
        // An empty PATH can trigger a shell's default search path. A real,
        // empty directory makes Git unavailable on every tested shell.
        let bin = fixture.dir.path().join("empty-bin");
        std::fs::create_dir(&bin).unwrap();
        assert_script_failure(
            fixture
                .probe(|command| {
                    command.env("PATH", &bin);
                })
                .unwrap_err(),
            127,
        );
    }
}

#[test]
fn malformed_configuration_is_not_an_absent_project() {
    for git in [false, true] {
        let fixture = Fixture::new(git, true);
        std::fs::write(fixture.dir.path().join("gitconfig"), "[broken\n").unwrap();
        assert_script_failure(fixture.probe(|_| {}).unwrap_err(), 128);
    }
    let fixture = Fixture::new(true, true);
    std::fs::write(fixture.checkout.join(".git/config"), "[broken\n").unwrap();
    assert_script_failure(fixture.probe(|_| {}).unwrap_err(), 128);
}

#[test]
fn unreadable_configuration_remains_a_discovery_failure() {
    let fixture = Fixture::new(true, true);
    let config = fixture.checkout.join(".git/config");
    std::fs::write(&config, "[user]\nname = fixture\n").unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o000)).unwrap();
    let result = fixture.probe(|_| {});
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_script_failure(result.unwrap_err(), 128);
}

#[test]
fn rejected_ownership_is_not_an_absent_project_or_a_beads_fallback() {
    let fixture = Fixture::new(true, true);
    let subdirectory = fixture.checkout.join("src");
    std::fs::create_dir(&subdirectory).unwrap();
    for checkout in [&fixture.checkout, &subdirectory] {
        assert_script_failure(
            fixture
                .probe_at(checkout, |command| {
                    command.env("GIT_TEST_ASSUME_DIFFERENT_OWNER", "1");
                })
                .unwrap_err(),
            128,
        );
    }
}

#[test]
fn broken_git_links_and_explicit_git_directories_remain_errors() {
    let fixture = Fixture::new(false, true);
    assert_script_failure(
        fixture
            .probe(|command| {
                command.env("GIT_DIR", fixture.dir.path().join("missing"));
            })
            .unwrap_err(),
        128,
    );
    std::os::unix::fs::symlink("missing", fixture.checkout.join(".git")).unwrap();
    assert_script_failure(fixture.probe(|_| {}).unwrap_err(), 128);
}

#[test]
fn failures_after_discovery_are_not_masked_by_pipelines_or_printf() {
    let fixture = Fixture::new(true, false);
    let bin = fixture.dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    test_executable::write(
        bin.join("git"),
        "#!/bin/sh\nif [ \"$*\" = \"$FAIL_COMMAND\" ]; then echo 'probe failed' >&2; exit 79; fi\nexec /usr/bin/git \"$@\"\n",
        0o755,
    ).unwrap();
    std::fs::write(
        fixture.checkout.join(".git/config"),
        "[remote \"origin\"]\nurl = https://github.com/example/project.git\n",
    )
    .unwrap();
    for failed in [
        "rev-parse --show-toplevel",
        "rev-parse --git-common-dir",
        "worktree list --porcelain",
        "remote",
        "remote get-url origin",
    ] {
        assert_script_failure(
            fixture
                .probe(|command| {
                    command.env("PATH", format!("{}:/usr/bin:/bin", bin.display()));
                    command.env("FAIL_COMMAND", failed);
                })
                .unwrap_err(),
            79,
        );
    }
}

#[test]
fn an_inaccessible_checkout_is_a_discovery_failure() {
    let fixture = Fixture::new(false, false);
    let error = fixture
        .probe_at(&fixture.dir.path().join("missing"), |_| {})
        .unwrap_err();
    assert!(matches!(error, Error::Script { .. }));
}
