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
        self.push_claiming(path, expect, input.len() as u64, input)
    }

    /// A push that says it sends `len` bytes and sends `input`.
    fn push_claiming(
        &self,
        path: &str,
        expect: Expect,
        len: u64,
        input: &[u8],
    ) -> std::result::Result<(), Error> {
        let command = self.command(&posix::push_script(path, expect, Access::Keep, len));
        push_with(command, input, &AtomicBool::new(false))
    }

    /// A push of a new file at `path` that takes the access of `source`, as
    /// a kept-both copy does.
    fn push_like(&self, path: &str, source: &str, input: &[u8]) -> std::result::Result<(), Error> {
        let script = posix::push_script(
            path,
            Expect::Absent,
            Access::Like(source),
            input.len() as u64,
        );
        push_with(self.command(&script), input, &AtomicBool::new(false))
    }

    /// Runs the scripts with a `chgrp` that always fails first on `PATH`, as
    /// for a user outside the file's group. Keep the returned folder alive.
    fn fail_chgrp(&mut self) -> tempfile::TempDir {
        self.shim("chgrp", "exit 1\n")
    }

    /// Runs the scripts with a `/bin/sh` script named `tool` holding `body`
    /// first on `PATH`. Keep the returned folder alive.
    fn shim(&mut self, tool: &str, body: &str) -> tempfile::TempDir {
        let bin = tempfile::tempdir().unwrap();
        let shim = bin.path().join(tool);
        std::fs::write(&shim, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut folders = vec![bin.path().to_owned()];
        folders.extend(std::env::split_paths(&path));
        self.path_env = Some(std::env::join_paths(folders).unwrap().into());
        bin
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

/// Runs `program` with `args` and returns its trimmed stdout.
fn output_of(program: &str, args: &[&str]) -> String {
    let output = Command::new(program).args(args).output().unwrap();
    assert!(output.status.success(), "{program} {args:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// Gives the file at `path` a group other than its own that this user may
/// give it: a supplementary group, or for root any other group. Each
/// candidate is tried in turn, since `chown` can refuse one `id -G` lists
/// (macOS's `everyone`, for one).
fn give_another_group(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    let gid = std::fs::metadata(path).unwrap().gid();
    let mut groups = output_of("id", &["-G"])
        .split_whitespace()
        .map(|group| group.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    if output_of("id", &["-u"]) == "0" {
        groups.extend([1, 2]);
    }
    groups.retain(|&group| group != gid);
    groups
        .iter()
        .copied()
        .find(|&group| std::os::unix::fs::chown(path, None, Some(group)).is_ok())
        .unwrap_or_else(|| {
            panic!(
                "chown refused every other group of this user ({groups:?}); run as root or with a \
                 supplementary group"
            )
        })
}

#[test]
fn push_keeps_an_existing_files_group() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    fixture.set_mode("file", 0o640);
    let group = give_another_group(&fixture.path("file"));
    let before = std::fs::metadata(fixture.path("file")).unwrap();
    fixture
        .push(&fixture.text("file"), Expect::Any, b"new")
        .unwrap();
    let after = std::fs::metadata(fixture.path("file")).unwrap();
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"new");
    assert_eq!(after.gid(), group);
    assert_eq!(fixture.mode("file"), 0o640);
    assert_ne!(
        after.ino(),
        before.ino(),
        "the copy was renamed over the file"
    );
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

// BSD `stat -f %Lp` reports only the rwx bits, so a BSD host drops the special bits on every rename.
#[cfg(target_os = "linux")]
#[test]
fn push_keeps_a_set_user_id_bit_it_can_keep() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    fixture.set_mode("file", 0o4755);
    fixture
        .push(&fixture.text("file"), Expect::Any, b"new")
        .unwrap();
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"new");
    assert_eq!(fixture.mode("file"), 0o4755);
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

/// A `stat` that reports a foreign owner for every path but the push's own
/// copy, so a test can act as a push over a file another account owns.
fn foreign_owner(fixture: &mut Fixture) -> tempfile::TempDir {
    let body = format!(
        "case \"$1 $2\" in\n'-c %u'|'-f %u')\n\
         for last; do :; done\n\
         case $last in */f.*) ;; *) printf '%s\\n' 4242; exit 0 ;; esac ;;\n\
         esac\nexec '{}' \"$@\"\n",
        real_tool("stat").display()
    );
    fixture.shim("stat", &body)
}

#[test]
fn push_clears_a_set_user_id_bit_when_the_owner_changes() {
    let mut fixture = Fixture::new();
    let _bin = foreign_owner(&mut fixture);
    std::fs::write(fixture.path("file"), b"old").unwrap();
    fixture.set_mode("file", 0o4755);
    fixture
        .push(&fixture.text("file"), Expect::Any, b"new")
        .unwrap();
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"new");
    assert_eq!(fixture.mode("file"), 0o755);
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

#[test]
fn push_like_clears_a_set_user_id_bit_when_the_source_has_another_owner() {
    let mut fixture = Fixture::new();
    let _bin = foreign_owner(&mut fixture);
    std::fs::write(fixture.path("source"), b"old").unwrap();
    fixture.set_mode("source", 0o4755);
    fixture
        .push_like(&fixture.text("new"), &fixture.text("source"), b"mine")
        .unwrap();
    assert_eq!(std::fs::read(fixture.path("new")).unwrap(), b"mine");
    assert_eq!(fixture.mode("new"), 0o755);
    assert_eq!(entries(fixture.home.path()), ["new", "source"]);
}

#[test]
fn push_clears_a_set_group_id_bit_when_it_cannot_keep_the_group() {
    let mut fixture = Fixture::new();
    let _bin = fixture.fail_chgrp();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    // `chown` clears the special bits, so the mode is set after the group.
    give_another_group(&fixture.path("file"));
    fixture.set_mode("file", 0o2755);
    fixture
        .push(&fixture.text("file"), Expect::Any, b"new")
        .unwrap();
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"new");
    assert_eq!(fixture.mode("file"), 0o755);
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

// BSD `stat -f %Lp` reports only the rwx bits, so a BSD host drops the special bits on every rename.
#[cfg(target_os = "linux")]
#[test]
fn push_keeps_a_set_group_id_bit_it_can_keep() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    // `chown` clears the special bits, so the mode is set after the group.
    let group = give_another_group(&fixture.path("file"));
    fixture.set_mode("file", 0o2755);
    fixture
        .push(&fixture.text("file"), Expect::Any, b"new")
        .unwrap();
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"new");
    assert_eq!(fixture.mode("file"), 0o2755);
    assert_eq!(
        std::fs::metadata(fixture.path("file")).unwrap().gid(),
        group
    );
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

#[test]
fn push_clears_a_set_user_id_bit_of_another_owners_file() {
    use std::os::unix::fs::MetadataExt;
    if output_of("id", &["-u"]) != "0" {
        eprintln!("skipped: needs root");
        return;
    }
    let fixture = Fixture::new();
    let path = fixture.path("file");
    std::fs::write(&path, b"old").unwrap();
    // `chown` clears the special bits, so the mode is set after the owner.
    std::os::unix::fs::chown(&path, Some(65534), None).unwrap();
    fixture.set_mode("file", 0o4755);
    fixture
        .push(&fixture.text("file"), Expect::Any, b"new")
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    assert_eq!(std::fs::metadata(&path).unwrap().uid(), 0);
    assert_eq!(fixture.mode("file"), 0o755);
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

#[test]
fn push_renames_without_the_group_only_when_group_and_other_bits_match() {
    use std::os::unix::fs::MetadataExt;
    let mut fixture = Fixture::new();
    let _bin = fixture.fail_chgrp();
    // Renamed, the file's old group falls into "other", so only identical
    // group and other bits keep who can read it.
    let cases = [
        (0o644, true),
        (0o640, false),
        (0o604, false),
        (0o664, false),
    ];
    for (mode, renamed) in cases {
        std::fs::write(fixture.path("file"), b"old").unwrap();
        fixture.set_mode("file", mode);
        let group = give_another_group(&fixture.path("file"));
        let before = std::fs::metadata(fixture.path("file")).unwrap();
        fixture
            .push(&fixture.text("file"), Expect::Any, b"new")
            .unwrap();
        let after = std::fs::metadata(fixture.path("file")).unwrap();
        assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"new");
        assert_eq!(fixture.mode("file"), mode);
        assert_eq!(after.ino() != before.ino(), renamed, "{mode:o} renamed");
        // Written in place, the file keeps its group; renamed, it takes the
        // user's, whose bits grant no more than everyone's.
        assert_eq!(after.gid() == group, !renamed, "{mode:o} group");
        assert_eq!(entries(fixture.home.path()), ["file"]);
        std::fs::remove_file(fixture.path("file")).unwrap();
    }
}

/// Makes the file at `path` unwritable to this user until dropped: by its
/// mode, or for root, whom no mode stops, with the immutable attribute.
struct Unwritable(std::path::PathBuf);

impl Unwritable {
    fn new(path: std::path::PathBuf) -> Self {
        if output_of("id", &["-u"]) == "0" {
            output_of("chattr", &["+i", path.to_str().unwrap()]);
        } else {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o440)).unwrap();
        }
        Self(path)
    }
}

impl Drop for Unwritable {
    fn drop(&mut self) {
        if output_of("id", &["-u"]) == "0" {
            output_of("chattr", &["-i", self.0.to_str().unwrap()]);
        }
    }
}

#[test]
fn push_refuses_an_in_place_write_to_a_file_it_cannot_write() {
    let mut fixture = Fixture::new();
    let _bin = fixture.fail_chgrp();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    fixture.set_mode("file", 0o640);
    give_another_group(&fixture.path("file"));
    let unwritable = Unwritable::new(fixture.path("file"));
    let pushed = fixture.push(&fixture.text("file"), Expect::Any, b"new");
    assert!(matches!(pushed, Err(Error::Access)), "{pushed:?}");
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"old");
    assert_eq!(entries(fixture.home.path()), ["file"]);
    drop(unwritable);
}

#[test]
fn push_like_gives_a_new_file_its_sources_access() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    std::fs::write(fixture.path(".env"), b"secret").unwrap();
    fixture.set_mode(".env", 0o600);
    fixture
        .push_like(
            &fixture.text(".env.conflict"),
            &fixture.text(".env"),
            b"mine",
        )
        .unwrap();
    assert_eq!(
        std::fs::read(fixture.path(".env.conflict")).unwrap(),
        b"mine"
    );
    assert_eq!(fixture.mode(".env.conflict"), 0o600);

    std::fs::write(fixture.path("shared"), b"team").unwrap();
    fixture.set_mode("shared", 0o640);
    let group = give_another_group(&fixture.path("shared"));
    fixture
        .push_like("~/shared.conflict", "~/shared", b"mine")
        .unwrap();
    let copy = std::fs::metadata(fixture.path("shared.conflict")).unwrap();
    assert_eq!(fixture.mode("shared.conflict"), 0o640);
    assert_eq!(copy.gid(), group);

    // A source that is gone gives no access to copy: the file stays private.
    fixture
        .push_like(
            &fixture.text("gone.conflict"),
            &fixture.text("gone"),
            b"mine",
        )
        .unwrap();
    assert_eq!(fixture.mode("gone.conflict"), 0o600);
    let mut names = vec![
        ".env",
        ".env.conflict",
        "gone.conflict",
        "shared",
        "shared.conflict",
    ];
    names.sort_unstable();
    assert_eq!(entries(fixture.home.path()), names);
}

#[test]
fn push_like_a_link_or_folder_leaves_the_new_file_private() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path("shared"), b"team").unwrap();
    fixture.set_mode("shared", 0o640);
    symlink(fixture.path("shared"), fixture.path("link")).unwrap();
    std::fs::create_dir(fixture.path("folder")).unwrap();
    fixture.set_mode("folder", 0o755);
    for source in ["link", "folder"] {
        let copy = format!("{source}.conflict");
        fixture
            .push_like(&fixture.text(&copy), &fixture.text(source), b"mine")
            .unwrap();
        assert_eq!(std::fs::read(fixture.path(&copy)).unwrap(), b"mine");
        assert_eq!(fixture.mode(&copy), 0o600, "{source}");
    }
}

/// A folder's ACL can let another user replace the file, so the push, which
/// the copy's inherited ACL sends in place, refuses instead.
#[test]
#[ignore = "needs setfacl and an ACL-capable filesystem"]
fn push_under_a_folders_acl_refuses_to_write_in_place() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    fixture.set_mode("file", 0o640);
    let home = fixture.home.path().to_str().unwrap();
    output_of("setfacl", &["-m", "u:nobody:rwx", home]);
    output_of("setfacl", &["-d", "-m", "u:nobody:r", home]);
    let file = fixture.text("file");
    let pushed = fixture.push(&file, Expect::Any, b"new");
    assert!(matches!(pushed, Err(Error::SharedFolder)), "{pushed:?}");
    assert_eq!(std::fs::read(&file).unwrap(), b"old");
    assert_eq!(fixture.mode("file"), 0o640);
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

#[test]
fn push_renames_over_a_file_whose_group_already_matches() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    let before = std::fs::metadata(fixture.path("file")).unwrap();
    fixture
        .push(&fixture.text("file"), Expect::Any, b"new")
        .unwrap();
    let after = std::fs::metadata(fixture.path("file")).unwrap();
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"new");
    assert_eq!(after.gid(), before.gid());
    assert_ne!(
        after.ino(),
        before.ino(),
        "the copy was renamed over the file"
    );
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

#[test]
#[ignore = "needs setfacl and an ACL-capable filesystem"]
fn push_overwrites_a_file_with_an_acl_in_place() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    fixture.set_mode("file", 0o640);
    let file = fixture.text("file");
    output_of("setfacl", &["-m", "u:nobody:r", &file]);
    let acl = output_of("getfacl", &["--omit-header", &file]);
    assert!(acl.contains("user:nobody:r--"), "{acl}");
    let before = std::fs::metadata(&file).unwrap();
    let content = payload(70_000);
    fixture.push(&file, Expect::Any, &content).unwrap();
    let after = std::fs::metadata(&file).unwrap();
    assert!(std::fs::read(&file).unwrap() == content);
    assert_eq!(after.ino(), before.ino(), "the file was written in place");
    assert_eq!(output_of("getfacl", &["--omit-header", &file]), acl);
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

#[test]
fn push_cut_off_short_of_its_length_replaces_nothing() {
    let fixture = Fixture::new();
    let first = payload(4096);
    std::fs::write(fixture.path("file"), &first).unwrap();
    let whole = payload(70_000);
    // The host sees a transfer that ended early only as the end of its input.
    let expects = [Expect::Sha256(sha256(&first)), Expect::Any];
    for expect in expects {
        let short = fixture.push_claiming(&fixture.text("file"), expect, 70_000, &whole[..1000]);
        assert!(matches!(short, Err(Error::Incomplete)), "{short:?}");
        assert!(std::fs::read(fixture.path("file")).unwrap() == first);
        assert_eq!(entries(fixture.home.path()), ["file"], "no temporary file");
    }
    let long = fixture.push_claiming(&fixture.text("file"), Expect::Any, 10, &whole[..11]);
    assert!(matches!(long, Err(Error::Incomplete)), "{long:?}");
    let fresh = fixture.push_claiming(&fixture.text("new"), Expect::Absent, 5, b"new");
    assert!(matches!(fresh, Err(Error::Incomplete)), "{fresh:?}");
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

#[test]
fn push_without_stat_still_refuses_a_cut_off_transfer() {
    let mut fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    let bin = tool_folder(WITHOUT_STAT);
    fixture.path_env = Some(bin.path().to_owned());
    let short = fixture.push_claiming(&fixture.text("file"), Expect::Any, 9, b"new");
    assert!(matches!(short, Err(Error::Incomplete)), "{short:?}");
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"old");
    assert_eq!(entries(fixture.home.path()), ["file"]);
}

/// Every tool a push runs except `stat` and the hashers.
const WITHOUT_STAT: &[&str] = &[
    "cat", "chmod", "id", "ls", "mktemp", "mv", "rm", "rmdir", "wc",
];

/// Where this machine keeps `tool`.
fn real_tool(tool: &str) -> std::path::PathBuf {
    ["/bin", "/usr/bin"]
        .iter()
        .map(|folder| Path::new(folder).join(tool))
        .find(|candidate| candidate.exists())
        .unwrap()
}

/// A folder holding links to just `tools`, from this machine's own folders.
fn tool_folder(tools: &[&str]) -> tempfile::TempDir {
    let bin = tempfile::tempdir().unwrap();
    for tool in tools {
        symlink(real_tool(tool), bin.path().join(tool)).unwrap();
    }
    bin
}

#[test]
fn push_without_stat_overwrites_in_place_and_keeps_the_mode() {
    use std::os::unix::fs::MetadataExt;
    let mut fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    fixture.set_mode("file", 0o755);
    // Only the SSH user can replace files in a 0700 folder, so writing in
    // place there is safe.
    std::fs::set_permissions(fixture.home.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let bin = tool_folder(WITHOUT_STAT);
    fixture.path_env = Some(bin.path().to_owned());
    let before = std::fs::metadata(fixture.path("file")).unwrap();
    let content = payload(70_000);
    fixture
        .push(&fixture.text("file"), Expect::Any, &content)
        .unwrap();
    let after = std::fs::metadata(fixture.path("file")).unwrap();
    assert!(std::fs::read(fixture.path("file")).unwrap() == content);
    assert_eq!(after.ino(), before.ino(), "the file was written in place");
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

/// Writes `file` holding `old` and a private `victim` file another user
/// would like the push to write through.
fn file_and_victim(fixture: &Fixture) {
    std::fs::write(fixture.path("file"), b"old").unwrap();
    fixture.set_mode("file", 0o644);
    std::fs::write(fixture.path("victim"), b"victim").unwrap();
    fixture.set_mode("victim", 0o600);
}

fn assert_victim_unchanged(fixture: &Fixture) {
    assert_eq!(std::fs::read(fixture.path("victim")).unwrap(), b"victim");
    assert_eq!(fixture.mode("victim"), 0o600);
}

/// The `ls` stand-in marks the target with an ACL, which sends the push in
/// place, and swaps it for a link to the victim as it lists it, as another
/// user who can rename entries in a 0777 folder can.
#[test]
fn push_refuses_to_write_in_place_in_a_folder_others_can_write() {
    let mut fixture = Fixture::new();
    file_and_victim(&fixture);
    std::fs::set_permissions(fixture.home.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
    let file = fixture.path("file");
    let body = format!(
        "case \"$*\" in\n*'-ld -- '*/file)\n  rm -f -- '{file}' && ln -s '{victim}' '{file}'\n  \
         echo '-rw-r--r--+ 1 0 0 3 Jan  1 00:00 file'\n  exit 0 ;;\nesac\nexec '{ls}' \"$@\"\n",
        file = file.display(),
        victim = fixture.path("victim").display(),
        ls = real_tool("ls").display(),
    );
    let _bin = fixture.shim("ls", &body);
    let pushed = fixture.push(&fixture.text("file"), Expect::Any, b"new");
    assert!(matches!(pushed, Err(Error::SharedFolder)), "{pushed:?}");
    assert_victim_unchanged(&fixture);
    assert_eq!(entries(fixture.home.path()), ["file", "victim"]);
}

/// The `mktemp` stand-in moves what it made aside and plants a link to the
/// victim at the name it prints, as another user can in a shared folder.
#[test]
fn push_never_writes_through_a_link_planted_at_its_temporary_name() {
    let mut fixture = Fixture::new();
    file_and_victim(&fixture);
    let body = format!(
        "made=$('{mktemp}' \"$@\") || exit 1\nmv -- \"$made\" '{aside}'\nln -s '{victim}' \
         \"$made\"\nprintf '%s\\n' \"$made\"\n",
        mktemp = real_tool("mktemp").display(),
        aside = fixture.path("aside").display(),
        victim = fixture.path("victim").display(),
    );
    let _bin = fixture.shim("mktemp", &body);
    let pushed = fixture.push(&fixture.text("file"), Expect::Any, b"new");
    assert!(pushed.is_err(), "{pushed:?}");
    assert_victim_unchanged(&fixture);
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"old");
}

/// The `chmod` stand-in first moves the push's temporary entry in the folder
/// aside and plants a link to the victim in its place, as another user can in
/// a shared folder.
#[test]
fn push_never_chmods_through_a_link_swapped_in_for_its_copy() {
    let mut fixture = Fixture::new();
    file_and_victim(&fixture);
    let body = format!(
        "for entry in '{home}'/.file.*; do\n  if [ -e \"$entry\" ] && [ ! -e '{aside}' ]; then\n    \
         mv -- \"$entry\" '{aside}' && ln -s '{victim}' \"$entry\"\n  fi\ndone\nexec '{chmod}' \
         \"$@\"\n",
        home = fixture.home.path().display(),
        aside = fixture.path("aside").display(),
        victim = fixture.path("victim").display(),
        chmod = real_tool("chmod").display(),
    );
    let _bin = fixture.shim("chmod", &body);
    let pushed = fixture.push(&fixture.text("file"), Expect::Any, b"new");
    assert!(pushed.is_ok(), "{pushed:?}");
    assert_victim_unchanged(&fixture);
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"new");
    assert_eq!(fixture.mode("file"), 0o644);
}

/// "Keep both" with a file that appears at the new name after the conflict
/// check: the `stat` stand-in plants it and fails, which would send the push
/// in place over it.
#[test]
fn push_absent_never_writes_in_place_over_a_file_that_appeared() {
    let mut fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    let copy = fixture.path("file.conflict");
    let body = format!(
        "[ -e '{copy}' ] || printf planted > '{copy}'\nexit 1\n",
        copy = copy.display()
    );
    let _bin = fixture.shim("stat", &body);
    let pushed = fixture.push_like(
        &fixture.text("file.conflict"),
        &fixture.text("file"),
        b"mine",
    );
    assert!(matches!(pushed, Err(Error::Conflict)), "{pushed:?}");
    assert_eq!(std::fs::read(&copy).unwrap(), b"planted");
    assert_eq!(entries(fixture.home.path()), ["file", "file.conflict"]);
}

/// Another user can swap the push's work folder for a link to one of the
/// SSH user's own private folders beside it, which passes every check on
/// the work folder; the `mktemp` stand-in hands that folder out for `-d`.
/// The push must open, replace, and remove only names it created there.
#[test]
fn push_never_touches_files_in_a_private_folder_handed_out_as_its_work_folder() {
    let mut fixture = Fixture::new();
    std::fs::write(fixture.path("file"), b"old").unwrap();
    std::fs::create_dir(fixture.path("sibling")).unwrap();
    fixture.set_mode("sibling", 0o700);
    let kept = ["f", "p", "q"];
    for name in kept {
        std::fs::write(fixture.path("sibling").join(name), name).unwrap();
    }
    let body = format!(
        "case \"$1\" in -d) printf '%s\\n' ./sibling; exit 0 ;; esac\nexec '{mktemp}' \"$@\"\n",
        mktemp = real_tool("mktemp").display(),
    );
    let _bin = fixture.shim("mktemp", &body);
    let pushed = fixture.push(&fixture.text("file"), Expect::Any, b"new");
    assert!(pushed.is_ok(), "{pushed:?}");
    assert_eq!(std::fs::read(fixture.path("file")).unwrap(), b"new");
    assert_eq!(entries(&fixture.path("sibling")), kept);
    for name in kept {
        let held = std::fs::read(fixture.path("sibling").join(name)).unwrap();
        assert_eq!(held, name.as_bytes(), "{name}");
    }
}

/// "Keep both" with a file that appears at the new name after the conflict
/// check and before the rename: the `mv` stand-in plants it on its first run.
#[test]
fn push_absent_never_renames_over_a_file_that_appeared() {
    let mut fixture = Fixture::new();
    let target = fixture.path("new");
    let body = format!(
        "[ -e '{target}' ] || printf planted > '{target}'\nexec '{mv}' \"$@\"\n",
        target = target.display(),
        mv = real_tool("mv").display(),
    );
    let _bin = fixture.shim("mv", &body);
    let pushed = fixture.push(&fixture.text("new"), Expect::Absent, b"mine");
    assert!(matches!(pushed, Err(Error::Conflict)), "{pushed:?}");
    assert_eq!(std::fs::read(&target).unwrap(), b"planted");
    assert_eq!(entries(fixture.home.path()), ["new"]);
}

/// A file whose name is as long as a name can be: the work folder's name
/// must not grow from it.
#[test]
fn push_writes_a_file_with_the_longest_name() {
    let fixture = Fixture::new();
    let name = "n".repeat(255);
    std::fs::write(fixture.path(&name), b"old").unwrap();
    fixture
        .push(&fixture.text(&name), Expect::Any, b"new")
        .unwrap();
    assert_eq!(std::fs::read(fixture.path(&name)).unwrap(), b"new");
    assert_eq!(entries(fixture.home.path()), [name]);
}
