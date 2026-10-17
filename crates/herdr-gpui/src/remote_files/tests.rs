use super::*;
use sha2::{Digest, Sha256};
use std::sync::Mutex;

/// The bytes [`notes`] serves.
pub(crate) const NOTES: &[u8] = b"# Notes\n";

/// A host that has the file asked for, holding [`NOTES`].
pub(crate) fn notes() -> herdr_client::Result<&'static [u8]> {
    Ok(NOTES)
}

/// A host that has nothing at the path asked for.
pub(crate) fn missing() -> herdr_client::Result<&'static [u8]> {
    Err(herdr_client::Error::RemoteFile(
        herdr_client::RemoteFileError::Missing,
    ))
}

/// An SSH host whose answer a test chooses, the fetches asked of it, and a
/// cache of its own in a temporary folder.
pub(crate) struct FakeHost {
    asked: Arc<Mutex<Vec<RemotePath>>>,
    cache: Arc<Cache>,
    _root: tempfile::TempDir,
}

impl FakeHost {
    pub(crate) fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let cache = Arc::new(Cache::create(root.path()).unwrap());
        Self {
            asked: Arc::default(),
            cache,
            _root: root,
        }
    }

    /// I/O that answers every fetch with `reply`.
    fn io(&self, reply: fn() -> herdr_client::Result<&'static [u8]>) -> Io {
        let asked = self.asked.clone();
        let cache = self.cache.clone();
        Io {
            cache: Arc::new(move || Ok(cache.clone())),
            fetch: Arc::new(move |remote, out| {
                asked.lock().unwrap().push(remote.clone());
                let bytes = reply()?;
                out.write_all(bytes).unwrap();
                Ok(Fetched {
                    bytes: bytes.len() as u64,
                    sha256: Sha256::digest(bytes).into(),
                })
            }),
        }
    }

    /// Has `files` fetch from this host, answering with `reply`.
    pub(crate) fn serve(
        &self,
        files: &mut RemoteFiles,
        reply: fn() -> herdr_client::Result<&'static [u8]>,
    ) {
        *files = RemoteFiles {
            io: self.io(reply),
            ..RemoteFiles::default()
        };
    }

    /// Every fetch asked of this host so far.
    pub(crate) fn asked(&self) -> Vec<RemotePath> {
        self.asked.lock().unwrap().clone()
    }
}

fn remote(path: &str) -> RemotePath {
    RemotePath {
        target: "me@devbox".into(),
        shell: RemoteShell::PowerShell,
        path: path.into(),
        line: None,
    }
}

#[test]
fn a_copy_is_named_after_the_host_s_file() {
    assert_eq!(cache::basename(r"C:\dev\notes.md"), "notes.md");
    assert_eq!(cache::basename("/home/me/src/main.rs"), "main.rs");
    assert_eq!(cache::basename(r"~\a/b\Makefile"), "Makefile");
    assert_eq!(cache::basename("README"), "README");
    for odd in [
        "/home/me/",
        r"C:\dev\",
        "/a/.",
        "/a/..",
        "",
        "/a/b\u{7}c",
        "~",
    ] {
        let expected = if odd == "~" { "~" } else { "file" };
        assert_eq!(cache::basename(odd), expected, "{odd:?}");
    }
}

#[test]
fn copies_are_kept_apart_by_host_and_path() {
    let host = FakeHost::new();
    let cache = &host.cache;
    let copy = cache.copy_path("me@devbox", "/a/notes.md");
    assert_eq!(copy, cache.copy_path("me@devbox", "/a/notes.md"), "stable");
    assert!(copy.starts_with(cache.launch()));
    assert_eq!(copy.file_name().unwrap(), "notes.md");
    let others = [
        cache.copy_path("me@other", "/a/notes.md"),
        cache.copy_path("me@devbox", "/b/notes.md"),
    ];
    for other in &others {
        assert_ne!(&copy, other);
        assert_eq!(other.file_name().unwrap(), "notes.md");
    }
    // Two folders of hex digits under the launch folder, then the file.
    let relative = copy.strip_prefix(cache.launch()).unwrap();
    let folders: Vec<_> = relative.iter().take(2).collect();
    for folder in folders {
        let folder = folder.to_str().unwrap();
        assert_eq!(folder.len(), 16, "{folder}");
        assert!(folder.chars().all(|c| c.is_ascii_hexdigit()), "{folder}");
    }
}

#[test]
fn a_launch_removes_unlocked_launch_folders_and_keeps_locked_ones() {
    let root = tempfile::tempdir().unwrap();
    let running = Cache::create(root.path()).unwrap();
    let crashed = root.path().join("1-dead-0");
    std::fs::create_dir(&crashed).unwrap();
    std::fs::write(crashed.join(".lock"), "").unwrap();
    std::fs::write(crashed.join("notes.md"), "old").unwrap();
    // A folder another launch is still creating has no lock file yet.
    let creating = root.path().join("2-new-0");
    std::fs::create_dir(&creating).unwrap();
    // Nor one whose lock is not yet held: it is still `.lock.new` then.
    let locking = root.path().join("3-new-0");
    std::fs::create_dir(&locking).unwrap();
    std::fs::write(locking.join(".lock.new"), "").unwrap();

    let fresh = Cache::create(root.path()).unwrap();
    assert!(
        locking.join(".lock.new").is_file(),
        "an unheld pending lock is kept"
    );
    assert!(!fresh.launch().join(".lock.new").exists());
    assert!(fresh.launch().join(".lock").is_file());
    assert!(!crashed.exists(), "a crashed launch's folder is removed");
    assert!(
        running.launch().join(".lock").is_file(),
        "a running one's is kept"
    );
    assert!(creating.is_dir());
    assert!(fresh.launch().is_dir());

    // Removing a cache releases its lock and its folder; removing it again
    // finds nothing to do.
    running.remove().unwrap();
    assert!(!running.launch().exists());
    running.remove().unwrap();
    assert!(fresh.launch().is_dir());
}

#[test]
fn a_fetch_writes_the_copy_privately_and_a_new_fetch_replaces_it() {
    let host = FakeHost::new();
    let done = fetch_copy(&host.io(notes), &remote(r"C:\dev\notes.md")).unwrap();
    let local = done.copy.local.clone();
    assert_eq!(std::fs::read(&local).unwrap(), NOTES);
    assert_eq!(done.copy.sha256, <[u8; 32]>::from(Sha256::digest(NOTES)));
    assert_eq!(done.copy.len, NOTES.len() as u64);
    assert_eq!(done.copy.shell, RemoteShell::PowerShell);
    assert_eq!(
        done.copy.modified,
        std::fs::metadata(&local).unwrap().modified().unwrap()
    );
    assert_eq!(done.key, ("me@devbox".into(), r"C:\dev\notes.md".into()));
    // A document opens itself.
    let opened = done.opened.unwrap().to_file_path().unwrap();
    assert_eq!(opened.file_name().unwrap(), "notes.md");
    assert_eq!(
        std::fs::read_dir(local.parent().unwrap()).unwrap().count(),
        1,
        "no .part left"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&local), 0o600);
        assert_eq!(mode(local.parent().unwrap()), 0o700);
        assert_eq!(mode(host.cache.launch()), 0o700);
    }

    fn newer() -> herdr_client::Result<&'static [u8]> {
        Ok(b"# Newer\n")
    }
    let again = fetch_copy(&host.io(newer), &remote(r"C:\dev\notes.md")).unwrap();
    assert_eq!(again.copy.local, local);
    assert_eq!(std::fs::read(&local).unwrap(), b"# Newer\n");
}

#[test]
fn a_script_opens_the_folder_holding_its_copy() {
    let host = FakeHost::new();
    let done = fetch_copy(&host.io(notes), &remote("/home/me/run.sh")).unwrap();
    let opened = done.opened.unwrap().to_file_path().unwrap();
    assert!(opened.is_dir(), "{}", opened.display());
    assert!(done.copy.local.starts_with(&opened));
}

#[test]
fn a_failed_fetch_leaves_the_older_copy_and_no_partial_file() {
    let host = FakeHost::new();
    let path = r"C:\dev\notes.md";
    let local = fetch_copy(&host.io(notes), &remote(path))
        .unwrap()
        .copy
        .local;

    let Err(error) = fetch_copy(&host.io(missing), &remote(path)) else {
        panic!("a missing file was fetched");
    };
    assert!(matches!(
        &error,
        Error::Fetch(herdr_client::Error::RemoteFile(
            herdr_client::RemoteFileError::Missing
        ))
    ));
    assert_eq!(
        crate::Error::from(error).to_string(),
        "Could not fetch the file: the file is not on the host"
    );
    assert_eq!(std::fs::read(&local).unwrap(), NOTES);
    assert_eq!(
        std::fs::read_dir(local.parent().unwrap()).unwrap().count(),
        1
    );
    assert_eq!(host.asked().len(), 2);
}

#[test]
fn a_fetch_into_an_unusable_cache_says_where() {
    let root = tempfile::tempdir().unwrap();
    let blocked = root.path().join("blocked");
    std::fs::write(&blocked, "").unwrap();
    let Err(error) = Cache::create(&blocked) else {
        panic!("a cache was made under a file");
    };
    assert!(
        matches!(&error, Error::Cache { path, .. } if *path == blocked),
        "{error:?}"
    );
}
