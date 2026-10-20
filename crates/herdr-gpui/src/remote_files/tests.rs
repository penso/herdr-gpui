use super::*;
use gpui::{Entity, TestAppContext, VisualTestContext};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

mod answers;
mod dismissed_conflict;
mod fetch_meta;
mod fetch_swap;
mod fetches;
mod folders;
mod queries;
mod saves;
mod windows_names;

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

/// A host whose file changed since it was fetched.
pub(crate) fn conflict() -> herdr_client::Result<()> {
    Err(herdr_client::Error::RemoteFile(
        herdr_client::RemoteFileError::Conflict,
    ))
}

/// A push the host received less of than it was told to expect.
pub(crate) fn cut_off() -> herdr_client::Result<()> {
    Err(herdr_client::Error::RemoteFile(
        herdr_client::RemoteFileError::Incomplete,
    ))
}

/// A fetch into a cache that keeps no edits, which always writes the copy.
fn fetch_fresh(io: &Io, owner: Owner, remote: &RemotePath) -> Result<Done> {
    fetch_copy(io, owner, remote, None, None).map(|fetch| match fetch {
        Fetch::Written(done) => *done,
        Fetch::KeptEdits => panic!("a fetch keeping no edits kept some"),
    })
}

/// How the fake host answers one push.
type PushReply = fn() -> herdr_client::Result<()>;

/// A push asked of the fake host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Received {
    pub(crate) path: String,
    pub(crate) expect: Expect,
    /// The host file whose access a new file was to take, if any.
    pub(crate) like: Option<String>,
    pub(crate) bytes: Vec<u8>,
}

/// How the fake host answers one folder probe.
pub(crate) type ProbeReply = fn() -> herdr_client::Result<ProbedFolder>;

/// How the fake host answers one file fetch.
pub(crate) type FetchReply = fn() -> herdr_client::Result<&'static [u8]>;

/// What a probe finds when a test sets no answer.
pub(crate) fn probed_folder() -> ProbedFolder {
    ProbedFolder {
        bytes: 4096,
        fingerprint: Some([7; 32]),
        top: "src".into(),
        links_skipped: 0,
    }
}

/// An SSH host whose answers a test chooses, the fetches and pushes asked
/// of it, and a cache of its own in a temporary folder.
pub(crate) struct FakeHost {
    asked: Arc<Mutex<Vec<RemotePath>>>,
    /// Bytes every folder fetch serves, once set.
    archive: Arc<Mutex<Option<Vec<u8>>>>,
    archives_asked: Arc<Mutex<Vec<RemotePath>>>,
    cache: Arc<Cache>,
    /// Bytes every fetch serves in place of its reply, once set.
    file: Arc<Mutex<Option<Vec<u8>>>>,
    /// Answers for the next fetches, in order; a fetch past them uses the
    /// reply it was made with.
    file_replies: Arc<Mutex<VecDeque<FetchReply>>>,
    /// Answers for the next probes, in order; a probe past them finds
    /// [`probed_folder`].
    probe_replies: Arc<Mutex<VecDeque<ProbeReply>>>,
    probes_asked: Arc<Mutex<Vec<RemotePath>>>,
    /// Answers for the next pushes, in order; a push past them succeeds.
    push_replies: Arc<Mutex<VecDeque<PushReply>>>,
    received: Arc<Mutex<Vec<Received>>>,
    _root: tempfile::TempDir,
}

impl FakeHost {
    pub(crate) fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let cache = Arc::new(Cache::create(root.path()).unwrap());
        Self {
            asked: Arc::default(),
            archive: Arc::default(),
            archives_asked: Arc::default(),
            cache,
            file: Arc::default(),
            file_replies: Arc::default(),
            probe_replies: Arc::default(),
            probes_asked: Arc::default(),
            push_replies: Arc::default(),
            received: Arc::default(),
            _root: root,
        }
    }

    /// I/O that answers every fetch with `reply`.
    pub(crate) fn io(&self, reply: FetchReply) -> Io {
        let asked = self.asked.clone();
        let archive = self.archive.clone();
        let archives_asked = self.archives_asked.clone();
        let cache = self.cache.clone();
        let file = self.file.clone();
        let file_replies = self.file_replies.clone();
        let probe_replies = self.probe_replies.clone();
        let probes_asked = self.probes_asked.clone();
        let replies = self.push_replies.clone();
        let received = self.received.clone();
        Io {
            step: Arc::new(|_, _| {}),
            cache: Arc::new(move || Ok(cache.clone())),
            folder_probe: Arc::new(move |remote| {
                probes_asked.lock().unwrap().push(remote.clone());
                let reply = probe_replies.lock().unwrap().pop_front();
                reply.map_or(Ok(probed_folder()), |reply| reply())
            }),
            folder_fetch: Arc::new(move |remote, out, max_bytes| {
                archives_asked.lock().unwrap().push(remote.clone());
                let bytes =
                    archive
                        .lock()
                        .unwrap()
                        .clone()
                        .ok_or(herdr_client::Error::RemoteFile(
                            herdr_client::RemoteFileError::Missing,
                        ))?;
                // As a real host does, an archive over the cap is refused
                // before any of it is sent.
                if bytes.len() as u64 > max_bytes {
                    return Err(herdr_client::Error::RemoteFile(
                        herdr_client::RemoteFileError::TooLarge { limit: max_bytes },
                    ));
                }
                out.write_all(&bytes).unwrap();
                Ok(Fetched {
                    bytes: bytes.len() as u64,
                    sha256: Sha256::digest(&bytes).into(),
                })
            }),
            fetch: Arc::new(move |remote, out| {
                asked.lock().unwrap().push(remote.clone());
                let bytes = match file.lock().unwrap().clone() {
                    Some(bytes) => bytes,
                    None => {
                        let queued = file_replies.lock().unwrap().pop_front();
                        queued.unwrap_or(reply)()?.to_vec()
                    }
                };
                out.write_all(&bytes).unwrap();
                Ok(Fetched {
                    bytes: bytes.len() as u64,
                    sha256: Sha256::digest(&bytes).into(),
                })
            }),
            push: Arc::new(move |remote, expect, access, len, input| {
                let mut bytes = Vec::new();
                input.read_to_end(&mut bytes).unwrap();
                let whole = bytes.len() as u64 == len;
                let like = match access {
                    RemoteFileAccess::Keep => None,
                    RemoteFileAccess::Like(source) => Some(source.to_owned()),
                };
                received.lock().unwrap().push(Received {
                    path: remote.path.clone(),
                    expect,
                    like,
                    bytes,
                });
                // As a real host does, a transfer short of its length fails.
                if !whole {
                    return cut_off();
                }
                let reply = replies.lock().unwrap().pop_front();
                reply.map_or(Ok(()), |reply| reply())
            }),
        }
    }

    /// Has later fetches serve `bytes`.
    pub(crate) fn change_file(&self, bytes: &[u8]) {
        *self.file.lock().unwrap() = Some(bytes.to_vec());
    }

    /// Answers the next pushes with `replies`, in order.
    pub(crate) fn answer_pushes(&self, replies: &[PushReply]) {
        self.push_replies.lock().unwrap().extend(replies);
    }

    /// Answers the next probes with `replies`, in order.
    pub(crate) fn answer_probes(&self, replies: &[ProbeReply]) {
        self.probe_replies.lock().unwrap().extend(replies);
    }

    /// Answers the next file fetches with `replies`, in order.
    pub(crate) fn answer_fetches(&self, replies: &[FetchReply]) {
        self.file_replies.lock().unwrap().extend(replies);
    }

    /// Has later folder fetches serve `bytes`.
    pub(crate) fn serve_archive(&self, bytes: &[u8]) {
        *self.archive.lock().unwrap() = Some(bytes.to_vec());
    }

    /// Every folder fetch asked of this host so far.
    pub(crate) fn archives(&self) -> Vec<RemotePath> {
        self.archives_asked.lock().unwrap().clone()
    }

    /// Every probe asked of this host so far.
    pub(crate) fn probes(&self) -> Vec<RemotePath> {
        self.probes_asked.lock().unwrap().clone()
    }

    /// Every push asked of this host so far, refused or not.
    pub(crate) fn received(&self) -> Vec<Received> {
        self.received.lock().unwrap().clone()
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

/// A window fetching from `host`.
fn window<'a>(
    host: &FakeHost,
    cx: &'a mut TestAppContext,
) -> (Entity<HerdrWindow>, &'a mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, _| host.serve(&mut view.remote_files, notes));
    (view, cx)
}

#[test]
fn a_copy_is_named_after_the_host_s_file() {
    let (windows, posix) = (RemoteShell::PowerShell, RemoteShell::Posix);
    assert_eq!(basename(r"C:\dev\notes.md", windows), "notes.md");
    assert_eq!(basename("/home/me/src/main.rs", posix), "main.rs");
    assert_eq!(basename("/home/me/src/main.rs", windows), "main.rs");
    assert_eq!(basename(r"~\a/b\Makefile", windows), "Makefile");
    assert_eq!(basename("README", posix), "README");
    assert_eq!(basename("/home/me/", posix), "me");
    assert_eq!(basename(r"C:\dev\", windows), "dev");
    assert_eq!(basename("/", posix), "file");
    for odd in ["/a/.", "/a/..", "", "/a/b\u{7}c", "~"] {
        let expected = if odd == "~" { "~" } else { "file" };
        assert_eq!(basename(odd, windows), expected, "{odd:?}");
    }
}

#[test]
fn a_copy_s_name_splits_at_the_host_s_own_separators() {
    assert_eq!(
        basename("/srv/foo\\bar.txt", RemoteShell::Posix),
        "foo\\bar.txt",
        "a backslash is part of a POSIX name"
    );
    assert_eq!(basename(r"C:\a\b.txt", RemoteShell::PowerShell), "b.txt");
    assert_eq!(basename("/srv/foo\\", RemoteShell::Posix), "foo\\");
    assert_eq!(basename("/srv/foo\\", RemoteShell::PowerShell), "foo");
}

#[test]
fn copies_are_kept_apart_by_host_and_path() {
    let host = FakeHost::new();
    let cache = &host.cache;
    let (owner, another) = (Owner::default(), Owner::default());
    let posix = RemoteShell::Posix;
    let copy = cache.copy_path(owner, "me@devbox", "/a/notes.md", posix);
    assert_eq!(
        copy,
        cache.copy_path(owner, "me@devbox", "/a/notes.md", posix),
        "stable"
    );
    assert!(copy.starts_with(cache.launch()));
    assert_eq!(copy.file_name().unwrap(), "notes.md");
    let others = [
        cache.copy_path(owner, "me@other", "/a/notes.md", posix),
        cache.copy_path(owner, "me@devbox", "/b/notes.md", posix),
        cache.copy_path(another, "me@devbox", "/a/notes.md", posix),
    ];
    for other in &others {
        assert_ne!(&copy, other);
        assert_eq!(other.file_name().unwrap(), "notes.md");
    }
    // The window's folder under the launch folder, two folders of hex
    // digits, then the file.
    let relative = copy.strip_prefix(cache.launch()).unwrap();
    let window = relative.iter().next().unwrap().to_str().unwrap();
    assert_eq!(window, format!("w{}", owner.0));
    let folders: Vec<_> = relative.iter().skip(1).take(2).collect();
    assert_eq!(folders.len(), 2);
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
    let owner = Owner::default();
    let done = fetch_fresh(&host.io(notes), owner, &remote(r"C:\dev\notes.md")).unwrap();
    let local = done.copy.local.clone();
    assert_eq!(std::fs::read(&local).unwrap(), NOTES);
    assert_eq!(
        done.copy.synced_sha256,
        <[u8; 32]>::from(Sha256::digest(NOTES))
    );
    assert_eq!(done.copy.synced_meta.len, NOTES.len() as u64);
    assert_eq!(done.copy.shell, RemoteShell::PowerShell);
    assert_eq!(
        done.copy.synced_meta.modified,
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
    let again = fetch_fresh(&host.io(newer), owner, &remote(r"C:\dev\notes.md")).unwrap();
    assert_eq!(again.copy.local, local);
    assert_eq!(std::fs::read(&local).unwrap(), b"# Newer\n");
}

#[test]
fn a_script_opens_the_folder_holding_its_copy() {
    let host = FakeHost::new();
    let done = fetch_fresh(
        &host.io(notes),
        Owner::default(),
        &remote("/home/me/run.sh"),
    )
    .unwrap();
    let opened = done.opened.unwrap().to_file_path().unwrap();
    assert!(opened.is_dir(), "{}", opened.display());
    // `opened` has symlinks resolved (macOS keeps temporary folders behind
    // `/var -> /private/var`), so compare both sides resolved.
    let holder = done.copy.local.parent().unwrap();
    assert_eq!(
        opened.canonicalize().unwrap(),
        holder.canonicalize().unwrap()
    );
}

#[test]
fn a_failed_fetch_leaves_the_older_copy_and_no_partial_file() {
    let host = FakeHost::new();
    let path = r"C:\dev\notes.md";
    let owner = Owner::default();
    let local = fetch_fresh(&host.io(notes), owner, &remote(path))
        .unwrap()
        .copy
        .local;

    let Err(error) = fetch_fresh(&host.io(missing), owner, &remote(path)) else {
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
