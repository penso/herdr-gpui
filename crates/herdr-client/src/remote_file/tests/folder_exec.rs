//! Runs the POSIX folder scripts under this machine's `/bin/sh`.

use super::super::folder as scripts;
use super::*;
use flate2::read::GzDecoder;
use std::io::Cursor;
use std::time::{Duration, SystemTime};

struct Fixture {
    home: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
        }
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.home.path().join(name)
    }

    fn text(&self, name: &str) -> String {
        self.path(name).to_str().unwrap().to_owned()
    }

    fn command(&self, script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]).env("HOME", self.home.path());
        command
    }

    fn probe(&self, path: &str) -> std::result::Result<ProbedFolder, Error> {
        probe_with(
            self.command(&scripts::posix_probe_script(path)),
            &AtomicBool::new(false),
        )
    }

    fn archive(
        &self,
        path: &str,
        max_bytes: u64,
    ) -> std::result::Result<(Vec<u8>, Fetched), Error> {
        let mut out = Vec::new();
        let limits = ScriptLimits {
            output: max_bytes,
            idle: ARCHIVE_IDLE,
        };
        let command = self.command(&scripts::posix_archive_script(path));
        let fetched = fetch_with_limits(command, &mut out, limits, &AtomicBool::new(false))?;
        Ok((out, fetched))
    }
}

/// Sets `path`'s modified time; `File::set_modified` needs a writable handle.
fn set_modified(path: &Path, time: SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

#[test]
fn probe_reports_the_tree_size_and_a_stable_fingerprint() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(fixture.path("tree/nested")).unwrap();
    std::fs::write(fixture.path("tree/a.txt"), [b'a'; 10]).unwrap();
    std::fs::write(fixture.path("tree/nested/b.bin"), payload(5000)).unwrap();
    // A listing that stops at the minute cannot tell these two edits apart.
    let first_edit = SystemTime::UNIX_EPOCH + Duration::from_secs(1_577_836_805);
    set_modified(&fixture.path("tree/a.txt"), first_edit);

    let first = fixture.probe(&fixture.text("tree")).unwrap();
    assert!(first.bytes >= 5010, "{first:?}");
    assert!(first.bytes <= 256 * 1024, "{first:?}");
    assert_eq!(first.bytes % 1024, 0);
    assert_eq!(first.top, "tree");
    let fingerprint = first.fingerprint.expect("this host has a SHA-256 tool");

    let home = fixture.probe("~/tree").unwrap();
    assert_eq!(home, first);

    let again = fixture.probe(&fixture.text("tree")).unwrap();
    assert_eq!(again, first);

    std::fs::write(fixture.path("tree/a.txt"), [b'b'; 10]).unwrap();
    set_modified(
        &fixture.path("tree/a.txt"),
        first_edit + Duration::from_secs(10),
    );
    let edited = fixture.probe(&fixture.text("tree")).unwrap();
    assert_ne!(
        edited.fingerprint,
        Some(fingerprint),
        "a same-length edit in the same minute changes the fingerprint"
    );

    std::fs::write(fixture.path("tree/added.txt"), b"added").unwrap();
    let changed = fixture.probe(&fixture.text("tree")).unwrap();
    assert_ne!(changed.fingerprint, Some(fingerprint));
}

#[test]
fn probe_and_archive_name_the_same_top_with_a_trailing_separator() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(fixture.path("v1.2")).unwrap();
    std::fs::write(fixture.path("v1.2/a.txt"), b"alpha").unwrap();
    let trailing = format!("{}/", fixture.text("v1.2"));

    let probed = fixture.probe(&trailing).unwrap();
    assert_eq!(probed.top, "v1.2");

    let (bytes, _) = fixture.archive(&trailing, MAX_REMOTE_FILE_BYTES).unwrap();
    let mut archive = tar::Archive::new(GzDecoder::new(Cursor::new(bytes)));
    let mut entries = archive.entries().unwrap();
    let first = entries.next().unwrap().unwrap();
    let path = first.path().unwrap().to_string_lossy().into_owned();
    assert_eq!(path.split('/').next(), Some(probed.top.as_str()), "{path}");

    let home = fixture.probe("~/").unwrap();
    assert_eq!(
        home.top,
        fixture.home.path().file_name().unwrap().to_str().unwrap()
    );
}

#[test]
fn probe_refuses_a_missing_path_and_a_file() {
    let fixture = Fixture::new();
    assert!(matches!(
        fixture.probe(&fixture.text("missing")),
        Err(Error::Missing)
    ));
    std::fs::write(fixture.path("file"), b"x").unwrap();
    assert!(matches!(
        fixture.probe(&fixture.text("file")),
        Err(Error::NotAFolder)
    ));
}

#[test]
fn archive_streams_the_tree_as_a_gzipped_tar() {
    let fixture = Fixture::new();
    let name = "it's a tree";
    std::fs::create_dir_all(fixture.path(&format!("{name}/nested"))).unwrap();
    std::fs::write(fixture.path(&format!("{name}/a.txt")), b"alpha").unwrap();
    std::fs::write(fixture.path(&format!("{name}/nested/b.bin")), payload(4096)).unwrap();

    let (bytes, fetched) = fixture
        .archive(&fixture.text(name), MAX_REMOTE_FILE_BYTES)
        .unwrap();
    assert_eq!(fetched.bytes, bytes.len() as u64);
    assert_eq!(fetched.sha256, sha256(&bytes));

    let mut archive = tar::Archive::new(GzDecoder::new(Cursor::new(bytes)));
    let mut files = Vec::new();
    for entry in archive.entries().unwrap() {
        let entry = entry.unwrap();
        if entry.header().entry_type().is_file() {
            files.push(entry.path().unwrap().to_string_lossy().into_owned());
        }
    }
    files.sort();
    assert_eq!(
        files,
        vec![format!("{name}/a.txt"), format!("{name}/nested/b.bin")]
    );
}

#[test]
fn archive_refuses_the_root_a_file_and_a_missing_path() {
    let fixture = Fixture::new();
    assert!(matches!(
        fixture.archive("/", MAX_REMOTE_FILE_BYTES),
        Err(Error::NotAFile)
    ));
    std::fs::write(fixture.path("file"), b"x").unwrap();
    assert!(matches!(
        fixture.archive(&fixture.text("file"), MAX_REMOTE_FILE_BYTES),
        Err(Error::NotAFile)
    ));
    assert!(matches!(
        fixture.archive(&fixture.text("missing"), MAX_REMOTE_FILE_BYTES),
        Err(Error::Missing)
    ));
}
