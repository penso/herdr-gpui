//! Test fixtures: a private home, a git repository, and a fake `herdr` that
//! records its arguments and answers as Herdr does, so steps run end to end
//! without a real daemon.

use crate::{herdr::Herdr, paths::Paths, settings::Settings, store::Store};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[path = "../../test-support/executable.rs"]
#[allow(clippy::disallowed_methods)]
mod executable;

pub struct Fixture {
    /// Held so the directory lives as long as the fixture.
    _dir: tempfile::TempDir,
    pub paths: Paths,
    pub herdr: Herdr,
    pub calls: PathBuf,
}

/// How the fake Herdr answers each command; `{worktree}` is replaced by a
/// directory the fixture creates.
const FAKE: &str = r#"#!/bin/sh
echo "$*" >> '{calls}'
case "$*" in
  *"--session hung status server"*) sleep 30 ;;
  *"status server"*) echo '{"status":"running","running":true}' ;;
  *"worktree create"*)
    mkdir -p '{worktree}'
    echo '{"id":"1","result":{"type":"worktree_created","worktree":{"path":"{worktree}","branch":"b"},"root_pane":{"pane_id":"w1p1"}}}' ;;
  *"agent start"*) echo '{"id":"1","result":{"type":"agent_started"}}' ;;
  *"agent prompt"*"slow"*"--wait"*) sleep 30; echo '{"id":"1","result":{"type":"agent_prompted"}}' ;;
  *"agent prompt"*) echo '{"id":"1","result":{"type":"agent_prompted"}}' ;;
  *"notification show"*) echo '{"id":"1","result":{"type":"notification_show","shown":true}}' ;;
  *"process-info --pane gone"*) echo '{"id":"1","error":{"code":"pane_not_found","message":"pane not found"}}' >&2; exit 1 ;;
  *"process-info --pane flaky"*) echo '{"id":"1","error":{"code":"server_unavailable","message":"no answer"}}' >&2; exit 1 ;;
  *"pane process-info"*) echo '{"id":"1","result":{"process_info":{"pane_id":"w1p1","shell_pid":1,"foreground_process_group_id":2}}}' ;;
  *"pane get"*) echo '{"id":"1","result":{"pane":{"agent":"claude","agent_status":"working"}}}' ;;
  *) echo '{"id":"1","error":{"code":"unknown_command","message":"not faked"}}' >&2; exit 1 ;;
esac
"#;

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let paths = Paths::under(&root);
        let calls = root.join("herdr-calls.txt");
        let fake = root.join("fake-herdr");
        let script = FAKE
            .replace("{calls}", &calls.to_string_lossy())
            .replace("{worktree}", &root.join("worktree").to_string_lossy());
        executable::write(&fake, script, 0o755).unwrap();
        Self {
            herdr: Herdr::new(Some(fake), None),
            _dir: dir,
            paths,
            calls,
        }
    }

    pub fn root(&self) -> PathBuf {
        self.paths.state.parent().unwrap().to_path_buf()
    }

    pub fn store(&self) -> Store {
        Store::open(&self.paths.database()).unwrap()
    }

    /// What the fake Herdr was asked, one line per call.
    pub fn calls(&self) -> String {
        fs::read_to_string(&self.calls).unwrap_or_default()
    }

    /// Settings that point at the fake Herdr.
    pub fn settings(&self) -> Settings {
        let fake = self.root().join("fake-herdr");
        fs::create_dir_all(&self.paths.config).unwrap();
        fs::write(
            self.paths.settings(),
            format!("herdr = '{}'\nmax_agents = 3\n", fake.display()),
        )
        .unwrap();
        Settings::read(&self.paths.settings()).unwrap()
    }

    /// A git repository on `main` with one commit and an automations folder.
    pub fn repo(&self) -> PathBuf {
        let repo = self.root().join("repo");
        fs::create_dir_all(repo.join(".herdr/automations")).unwrap();
        fs::write(repo.join("README.md"), "hello\n").unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["add", "README.md"]);
        git(&repo, &["commit", "-q", "-m", "first"]);
        repo
    }

    /// Writes `.herdr/automations/NAME.luau` with caps and `body`.
    pub fn script(&self, repo: &Path, name: &str, body: &str) -> PathBuf {
        let path = repo.join(".herdr/automations").join(format!("{name}.luau"));
        fs::write(
            &path,
            format!(
                "herdr.caps {{ agents_per_run = 2, tokens_per_day = 1_000_000, wall_time = \"1h\" }}\n{body}"
            ),
        )
        .unwrap();
        path
    }
}

pub fn git(repo: &Path, args: &[&str]) {
    let status = herdr_client::new_command("git")
        .arg("-C")
        .arg(repo)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}
