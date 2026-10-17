//! Runs the POSIX scripts under this machine's `/bin/sh` through the same
//! runner an SSH host's scripts use.

use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

struct Fixture {
    home: tempfile::TempDir,
    /// The child's `PATH`; `None` keeps this process's.
    path_env: Option<std::path::PathBuf>,
    /// The umask the script starts with, as an SSH login sets one.
    umask: &'static str,
}

impl Fixture {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
            path_env: None,
            umask: "022",
        }
    }

    fn mode(&self, name: &str) -> u32 {
        std::fs::metadata(self.path(name))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777
    }

    fn set_mode(&self, name: &str, mode: u32) {
        std::fs::set_permissions(self.path(name), std::fs::Permissions::from_mode(mode)).unwrap();
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.home.path().join(name)
    }

    fn text(&self, name: &str) -> String {
        self.path(name).to_str().unwrap().to_owned()
    }

    fn command(&self, script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", &format!("umask {}\n{script}", self.umask)])
            .env("HOME", self.home.path());
        if let Some(path) = &self.path_env {
            command.env("PATH", path);
        }
        command
    }

    fn fetch(&self, path: &str, max_bytes: u64) -> std::result::Result<(Vec<u8>, Fetched), Error> {
        let mut out = Vec::new();
        let command = self.command(&posix::fetch_script(path, max_bytes));
        let fetched = fetch_with(command, &mut out, max_bytes, &AtomicBool::new(false))?;
        Ok((out, fetched))
    }

    fn push(&self, path: &str, expect: Expect, input: &[u8]) -> std::result::Result<(), Error> {
        let command = self.command(&posix::push_script(path, expect));
        push_with(command, input, &AtomicBool::new(false))
    }
}

#[test]
fn fetch_streams_a_binary_file_exactly() {
    let fixture = Fixture::new();
    let original = payload(3 * 1024 * 1024 + 17);
    std::fs::write(fixture.path(HOSTILE_NAME), &original).unwrap();
    let (bytes, fetched) = fixture
        .fetch(&fixture.text(HOSTILE_NAME), MAX_REMOTE_FILE_BYTES)
        .unwrap();
    assert!(bytes == original, "fetched bytes differ");
    assert_eq!(fetched.bytes, original.len() as u64);
    assert_eq!(fetched.sha256, sha256(&original));

    let (home, _) = fixture
        .fetch(&format!("~/{HOSTILE_NAME}"), MAX_REMOTE_FILE_BYTES)
        .unwrap();
    assert!(home == original, "home-relative bytes differ");
}

#[test]
fn fetch_follows_a_symlink() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path("target"), b"linked").unwrap();
    symlink(fixture.path("target"), fixture.path("link")).unwrap();
    let (bytes, _) = fixture.fetch(&fixture.text("link"), 100).unwrap();
    assert_eq!(bytes, b"linked");
}

#[test]
fn fetch_refuses_missing_folders_devices_and_oversize_files() {
    let fixture = Fixture::new();
    assert!(matches!(
        fixture.fetch(&fixture.text("missing"), 10),
        Err(Error::Missing)
    ));
    symlink(fixture.path("nowhere"), fixture.path("dangling")).unwrap();
    assert!(matches!(
        fixture.fetch(&fixture.text("dangling"), 10),
        Err(Error::Missing)
    ));
    std::fs::create_dir(fixture.path("folder")).unwrap();
    assert!(matches!(
        fixture.fetch(&fixture.text("folder"), 10),
        Err(Error::NotAFile)
    ));
    assert!(matches!(
        fixture.fetch("/dev/null", 10),
        Err(Error::NotAFile)
    ));
    std::fs::write(fixture.path("big"), payload(11)).unwrap();
    assert!(matches!(
        fixture.fetch(&fixture.text("big"), 10),
        Err(Error::TooLarge { limit: 10 })
    ));
    assert!(fixture.fetch(&fixture.text("big"), 11).is_ok());
}

#[test]
fn push_replaces_a_file_that_still_matches_its_hash() {
    let fixture = Fixture::new();
    let path = fixture.text(HOSTILE_NAME);
    let first = payload(4096);
    std::fs::write(&path, &first).unwrap();
    let second = payload(70_000).into_iter().rev().collect::<Vec<_>>();
    fixture
        .push(&path, Expect::Sha256(sha256(&first)), &second)
        .unwrap();
    assert!(std::fs::read(&path).unwrap() == second);
    assert_eq!(entries(fixture.home.path()), [HOSTILE_NAME]);

    let stale = fixture.push(&path, Expect::Sha256(sha256(&first)), b"stale");
    assert!(matches!(stale, Err(Error::Conflict)));
    assert!(std::fs::read(&path).unwrap() == second);
    assert_eq!(entries(fixture.home.path()), [HOSTILE_NAME]);

    let missing = fixture.push(&fixture.text("missing"), Expect::Sha256(sha256(b"")), b"x");
    assert!(matches!(missing, Err(Error::Conflict)));
    assert_eq!(entries(fixture.home.path()), [HOSTILE_NAME]);
}

#[test]
fn push_absent_creates_with_the_hosts_default_mode_and_refuses_an_existing_file() {
    let mut fixture = Fixture::new();
    fixture
        .push(&format!("~/{HOSTILE_NAME}"), Expect::Absent, b"new")
        .unwrap();
    let path = fixture.path(HOSTILE_NAME);
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    assert_eq!(fixture.mode(HOSTILE_NAME), 0o644);

    fixture.umask = "0027";
    fixture
        .push(&fixture.text("other"), Expect::Any, b"other")
        .unwrap();
    assert_eq!(fixture.mode("other"), 0o640);
    std::fs::remove_file(fixture.path("other")).unwrap();

    let again = fixture.push(&fixture.text(HOSTILE_NAME), Expect::Absent, b"again");
    assert!(matches!(again, Err(Error::Conflict)));
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    assert_eq!(entries(fixture.home.path()), [HOSTILE_NAME]);

    symlink(fixture.path("nowhere"), fixture.path("dangling")).unwrap();
    let dangling = fixture.push(&fixture.text("dangling"), Expect::Absent, b"x");
    assert!(matches!(dangling, Err(Error::Symlink)));
}

#[test]
fn push_keeps_an_existing_files_mode() {
    let fixture = Fixture::new();
    for mode in [0o755, 0o644] {
        std::fs::write(fixture.path("file"), b"old").unwrap();
        fixture.set_mode("file", mode);
        let expect = Expect::Sha256(sha256(b"old"));
        fixture.push(&fixture.text("file"), expect, b"new").unwrap();
        assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"new");
        assert_eq!(fixture.mode("file"), mode);
        fixture
            .push(&fixture.text("file"), Expect::Any, b"any")
            .unwrap();
        assert_eq!(fixture.mode("file"), mode);
    }
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

/// A folder holding links to just `tools`, from this machine's own folders.
fn tool_folder(tools: &[&str]) -> tempfile::TempDir {
    let bin = tempfile::tempdir().unwrap();
    for tool in tools {
        let found = ["/bin", "/usr/bin"]
            .iter()
            .map(|folder| Path::new(folder).join(tool))
            .find(|candidate| candidate.exists())
            .unwrap();
        symlink(found, bin.path().join(tool)).unwrap();
    }
    bin
}

#[test]
fn push_without_stat_overwrites_in_place_and_keeps_the_mode() {
    let mut fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    fixture.set_mode("file", 0o755);
    let bin = tool_folder(&["cat", "chmod", "mktemp", "mv", "rm"]);
    fixture.path_env = Some(bin.path().to_owned());
    let content = payload(70_000);
    fixture
        .push(&fixture.text("file"), Expect::Any, &content)
        .unwrap();
    assert!(std::fs::read(fixture.path("file")).unwrap() == content);
    assert_eq!(fixture.mode("file"), 0o755);
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

#[test]
fn push_any_overwrites_but_never_replaces_a_symlink_or_folder() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    fixture
        .push(&fixture.text("file"), Expect::Any, b"any")
        .unwrap();
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"any");

    symlink(fixture.path("file"), fixture.path("link")).unwrap();
    for expect in [Expect::Any, Expect::Sha256(sha256(b"any"))] {
        let linked = fixture.push(&fixture.text("link"), expect, b"through");
        assert!(matches!(linked, Err(Error::Symlink)));
    }
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"any");
    assert!(
        std::fs::symlink_metadata(fixture.path("link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );

    std::fs::create_dir(fixture.path("folder")).unwrap();
    let folder = fixture.push(&fixture.text("folder"), Expect::Any, b"x");
    assert!(matches!(folder, Err(Error::NotAFile)));
    assert_eq!(entries(fixture.home.path()), ["file", "folder", "link"]);
}

#[test]
fn push_without_a_hasher_refuses_before_writing() {
    let mut fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    // Every tool the script runs before choosing a hasher is a shell builtin,
    // so an empty PATH hides all three hashers without breaking the script.
    let bin = tempfile::tempdir().unwrap();
    fixture.path_env = Some(bin.path().to_owned());
    let pushed = fixture.push(
        &fixture.text("file"),
        Expect::Sha256(sha256(b"old")),
        b"new",
    );
    assert!(matches!(pushed, Err(Error::NoHasher)));
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"old");
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

#[test]
fn push_of_unread_output_is_a_script_failure() {
    let fixture = Fixture::new();
    let command = fixture.command("echo stray");
    let pushed = push_with(command, &b""[..], &AtomicBool::new(false));
    let error = pushed.unwrap_err();
    assert!(matches!(
        script_error(&error),
        Some(crate::Error::ScriptOutputLimit)
    ));
}
