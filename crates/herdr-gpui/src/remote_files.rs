//! Files on SSH hosts that a link-modifier click opens here. The click
//! fetches the host's file over the endpoint's SSH trust into a private
//! cache folder, then opens the copy with the system's default application
//! under the same rule as a local path: a document itself, and anything
//! else the folder holding it. The window records each copy it made, keyed
//! by host and path, and the copies are removed when the app quits.
//!
//! While the window holds copies it watches them for saves (`watch`), and
//! sends a save back to the host under the rules in `sync`: it asks first,
//! unless the user chose to upload that copy's saves without asking, and a
//! host's file that changed since it was synced is never replaced without
//! the user's say.
//!
//! Every fetch, push, and disk access runs on the background executor; the
//! UI thread only starts them and applies their results.

mod cache;
mod push;
mod sync;
mod uploads;
mod watch;

use crate::window::{Flash, HerdrWindow, RemotePath};
use gpui::{App, Context, Task};
use herdr_client::{Expect, Fetched, MAX_REMOTE_FILE_BYTES, RemoteFileAccess, RemoteShell};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};

pub(crate) use cache::{Cache, basename};
pub(crate) use sync::{Answer, Policy, Resolution, SyncState};

/// Bytes in the megabyte that [`Error::TooLarge`] reports.
const MB: u64 = 1024 * 1024;

/// Why a remote file was not fetched, opened, or uploaded.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not keep the file's access; nothing was changed.")]
    Access,
    #[error("Could not use the remote file cache at {}: {source}", path.display())]
    Cache {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Could not fetch the file: {0}")]
    Fetch(#[source] herdr_client::Error),
    #[error("Could not keep both: every name up to {last} is taken on the host")]
    NamesTaken { last: String },
    #[error("No state folder for the remote file cache: set XDG_STATE_HOME, HOME, or LOCALAPPDATA")]
    NoStateDir,
    #[error("Could not upload the file: {0}")]
    Push(#[source] herdr_client::Error),
    #[error(
        "Other users can replace files in that folder on the host, so it was not written in \
         place; nothing was changed."
    )]
    SharedFolder,
    #[error("Not uploaded: the copy is larger than {} MB", .limit.div_ceil(MB))]
    TooLarge { limit: u64 },
    #[error("Not uploaded: the copy kept changing while it was read; save it again")]
    Unsettled,
    // keep-sorted end
}

impl Error {
    /// Whether the host refused a push because its file was not what the
    /// push expected.
    fn is_conflict(&self) -> bool {
        matches!(
            self,
            Self::Push(herdr_client::Error::RemoteFile(
                herdr_client::RemoteFileError::Conflict
            ))
        )
    }
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

/// A copy's SSH target and host path.
pub(crate) type Key = (String, String);

/// Streams the host's file `RemotePath` names into the writer. Blocking.
pub(crate) type Fetcher = Arc<
    dyn Fn(&RemotePath, &mut (dyn Write + Send)) -> herdr_client::Result<Fetched> + Send + Sync,
>;

/// Replaces the host's file `RemotePath` names with the reader's bytes, of
/// which there are exactly the given count, while it meets the expectation,
/// giving a new file the access named. Blocking.
pub(crate) type Pusher = Arc<
    dyn Fn(
            &RemotePath,
            Expect,
            RemoteFileAccess<'_>,
            u64,
            &mut (dyn Read + Send),
        ) -> herdr_client::Result<()>
        + Send
        + Sync,
>;

/// The cache copies are written to. Blocking.
pub(crate) type CacheSource = Arc<dyn Fn() -> Result<Arc<Cache>> + Send + Sync>;

/// The I/O fetches and pushes do: SSH and the process's cache in the app,
/// and fakes in tests.
#[derive(Clone)]
pub(crate) struct Io {
    cache: CacheSource,
    fetch: Fetcher,
    push: Pusher,
    /// Runs at each step of putting a fetched copy in place, with the
    /// copy's path, so a test can save to it then.
    #[cfg(test)]
    step: StepHook,
}

/// What a test runs at each [`Step`], with the copy's path.
#[cfg(test)]
type StepHook = Arc<dyn Fn(Step, &Path) + Send + Sync>;

/// A step of putting a fetched copy in place that a test can save at.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// The copy was checked for saves since it was synced.
    Checked,
    /// The copy was moved aside, leaving its place empty.
    MovedAside,
    /// The host's bytes are at the copy's place.
    Placed,
}

impl Default for Io {
    fn default() -> Self {
        Self {
            #[cfg(test)]
            step: Arc::new(|_, _| {}),
            cache: Arc::new(cache::shared),
            // The scripts' idle timeout ends a transfer that stalls.
            fetch: Arc::new(|remote, out| {
                herdr_client::fetch_remote_file(
                    remote.remote_file(),
                    out,
                    MAX_REMOTE_FILE_BYTES,
                    &AtomicBool::new(false),
                )
            }),
            push: Arc::new(|remote, expect, access, len, input| {
                herdr_client::push_remote_file(
                    remote.remote_file(),
                    expect,
                    access,
                    len,
                    input,
                    &AtomicBool::new(false),
                )
            }),
        }
    }
}

impl std::fmt::Debug for Io {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Io").finish_non_exhaustive()
    }
}

/// A local copy of a host's file, and where it stands against the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CachedCopy {
    pub(crate) local: PathBuf,
    pub(crate) policy: Policy,
    pub(crate) shell: RemoteShell,
    pub(crate) state: SyncState,
    /// The copy's metadata when it last matched the host.
    pub(crate) synced_meta: watch::Meta,
    /// The host's bytes as last fetched or pushed.
    pub(crate) synced_sha256: sync::Sha256,
    watch: watch::Watch,
}

/// One copy as `RemoteFiles::list` shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Listed<'a> {
    pub(crate) target: &'a str,
    pub(crate) remote_path: &'a str,
    pub(crate) local: &'a Path,
    pub(crate) state: SyncState,
    pub(crate) policy: Policy,
}

/// Which question a copy waits to ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptKind {
    Upload,
    Conflict,
}

/// A question waiting for the window to have no page open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Prompt {
    pub(crate) key: Key,
    pub(crate) kind: PromptKind,
}

/// The window whose copies a folder of the cache holds. Each window keeps
/// its own copies, so a fetch in one never writes over another's edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Owner(u64);

impl Default for Owner {
    /// An owner no other window of this process has.
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// The copies one window fetched, by SSH target and host path.
#[derive(Debug, Default)]
pub(crate) struct RemoteFiles {
    /// The cache the copies are in, once one was written.
    cache: Option<Arc<Cache>>,
    copies: HashMap<Key, CachedCopy>,
    /// The copies a click's fetch or a conflict's refetch is writing, which
    /// nothing else touches until it is done.
    fetching: HashSet<Key>,
    io: Io,
    /// When the last watch run started.
    last_watch: Option<Instant>,
    /// The folder of the cache this window's copies are in.
    owner: Owner,
    prompts: VecDeque<Prompt>,
    /// Whether a watch run is underway.
    watching: bool,
}

impl RemoteFiles {
    /// The copy last fetched of `path` on `target`.
    pub(crate) fn copy(&self, target: &str, path: &str) -> Option<&CachedCopy> {
        self.copies.get(&(target.to_owned(), path.to_owned()))
    }

    /// Every copy, with where it stands against its host.
    pub(crate) fn list(&self) -> impl Iterator<Item = Listed<'_>> {
        self.copies.iter().map(|((target, path), copy)| Listed {
            target,
            remote_path: path,
            local: &copy.local,
            state: copy.state,
            policy: copy.policy,
        })
    }

    /// Whether a click on `remote` fetches it again rather than reopening
    /// the copy already here. A synced copy is fetched again, since the
    /// host's bytes may be newer; one holding edits not yet on the host is
    /// reopened, keeping them.
    pub(crate) fn fetch_again(&self, remote: &RemotePath) -> bool {
        self.copy(&remote.target, &remote.path)
            .is_none_or(|copy| copy.state == SyncState::Synced)
    }

    /// Keeps a fetched copy, synced, with the upload policy an earlier copy
    /// of the same file had.
    fn record(&mut self, done: Done) {
        self.cache = Some(done.cache);
        let mut copy = done.copy;
        if let Some(earlier) = self.copies.get(&done.key) {
            copy.policy = earlier.policy;
        }
        self.copies.insert(done.key, copy);
    }

    /// Steps `key`'s copy through `event`, returning what to do next.
    fn handle(&mut self, key: &Key, event: sync::Event) -> sync::Action {
        let Some(copy) = self.copies.get_mut(key) else {
            return sync::Action::Nothing;
        };
        let (state, policy, action) = sync::step(copy.state, copy.policy, event);
        copy.state = state;
        copy.policy = policy;
        action
    }

    /// Records that the host now holds what `pushed` sent.
    fn synced(&mut self, key: &Key, pushed: push::Pushed) {
        if let Some(copy) = self.copies.get_mut(key) {
            copy.synced_sha256 = pushed.sha256;
            copy.synced_meta = pushed.meta;
            copy.watch = watch::Watch::default();
        }
    }

    /// Queues `prompt` unless it already waits.
    fn enqueue(&mut self, prompt: Prompt) {
        if !self.prompts.contains(&prompt) {
            self.prompts.push_back(prompt);
        }
    }

    /// Whether `prompt`'s copy is still in the state its question asks about.
    fn applies(&self, prompt: &Prompt) -> bool {
        self.copies.get(&prompt.key).is_some_and(|copy| {
            matches!(
                (prompt.kind, copy.state),
                (PromptKind::Upload, SyncState::Asking { .. })
                    | (PromptKind::Conflict, SyncState::Conflict { .. })
            )
        })
    }

    /// The first waiting question that still applies, dropping the ones its
    /// copy moved past.
    fn next_prompt(&mut self) -> Option<Prompt> {
        while let Some(prompt) = self.prompts.front() {
            if self.applies(prompt) {
                return Some(prompt.clone());
            }
            self.prompts.pop_front();
        }
        None
    }

    /// Queues again the conflict question of `key`'s copy when the copy is
    /// in conflict and the question is neither waiting nor `shown`, as after
    /// the user closed it without an answer.
    fn reask_conflict(&mut self, key: Key, shown: Option<&Prompt>) {
        let prompt = Prompt {
            key,
            kind: PromptKind::Conflict,
        };
        if self.applies(&prompt) && shown != Some(&prompt) {
            self.enqueue(prompt);
        }
    }

    /// Queues again the upload question of a copy still asking whose
    /// question is neither waiting nor `shown`: another page replaced it
    /// before the user answered.
    fn requeue_unanswered(&mut self, shown: Option<&Prompt>) {
        let orphans: Vec<Key> = self
            .copies
            .iter()
            .filter(|(_, copy)| matches!(copy.state, SyncState::Asking { .. }))
            .map(|(key, _)| key.clone())
            .filter(|key| {
                let prompt = Prompt {
                    key: key.clone(),
                    kind: PromptKind::Upload,
                };
                shown != Some(&prompt) && !self.prompts.contains(&prompt)
            })
            .collect();
        for key in orphans {
            self.prompts.push_back(Prompt {
                key,
                kind: PromptKind::Upload,
            });
        }
    }

    /// The copies a watch run looks at: all but those a transfer is
    /// rewriting or reading, or a fetch is writing.
    fn probes(&self) -> Vec<watch::Probe> {
        self.copies
            .iter()
            .filter(|(key, copy)| !transferring(copy.state) && !self.fetching.contains(*key))
            .map(|(key, copy)| watch::Probe {
                key: key.clone(),
                local: copy.local.clone(),
                synced: copy.synced_meta,
                watch: copy.watch,
            })
            .collect()
    }

    /// Applies a watch run's findings, returning each settled save's next
    /// action.
    fn apply_scan(&mut self, seen: Vec<watch::Seen>) -> Vec<(Key, sync::Action)> {
        let mut actions = Vec::new();
        for seen in seen {
            // A fetch writing the copy now owns it until it is done.
            if self.fetching.contains(&seen.key) {
                continue;
            }
            let Some(copy) = self.copies.get_mut(&seen.key) else {
                continue;
            };
            // Synced again or mid-transfer since the run began: its findings
            // describe a copy that is no longer there.
            if copy.synced_meta != seen.synced || transferring(copy.state) {
                continue;
            }
            let event = match seen.change {
                watch::Change::Skipped => None,
                watch::Change::Quiet => {
                    copy.watch.pending = None;
                    None
                }
                watch::Change::Pending(meta) => {
                    copy.watch.pending = Some(meta);
                    None
                }
                watch::Change::Settled { meta, sha256 } => Some(settle(copy, meta, sha256)),
            };
            if let Some(event) = event {
                actions.push((seen.key.clone(), self.handle(&seen.key, event)));
            }
        }
        actions
    }

    /// Forgets the copies and removes the cache folder holding them, which
    /// every window of the process shares, off the UI thread.
    pub(crate) fn remove_copies(&mut self, cx: &App) -> Task<()> {
        self.copies.clear();
        self.prompts.clear();
        let cache = self.cache.take();
        cx.background_executor().spawn(async move {
            if let Some(cache) = cache
                && let Err(error) = cache.remove()
            {
                tracing::warn!(%error, "Remote file copies not removed");
            }
        })
    }

    /// Lets the next poll start a watch run at once.
    #[cfg(test)]
    pub(crate) fn rewind_watch(&mut self) {
        self.last_watch = None;
    }
}

/// Whether a push or fetch is reading or rewriting the copy.
fn transferring(state: SyncState) -> bool {
    matches!(
        state,
        SyncState::Uploading { .. } | SyncState::Refetching { .. }
    )
}

/// Records a settled save of `copy` and names it: bytes the host already
/// has are only a touch, which moves the synced metadata along.
fn settle(copy: &mut CachedCopy, meta: watch::Meta, sha256: sync::Sha256) -> sync::Event {
    copy.watch.pending = None;
    if sha256 == copy.synced_sha256 {
        copy.synced_meta = meta;
        copy.watch.handled = None;
        sync::Event::Reverted
    } else {
        copy.watch.handled = Some(meta);
        sync::Event::Saved(sha256)
    }
}

/// A finished fetch: the copy written, and what a click opens for it.
struct Done {
    key: Key,
    copy: CachedCopy,
    cache: Arc<Cache>,
    /// The copy, or the folder holding it when it is not a document.
    opened: Option<url::Url>,
}

/// What a fetch did with the copy.
enum Fetch {
    Written(Box<Done>),
    /// The copy was saved since it was synced, so the host's bytes were not
    /// written over it.
    KeptEdits,
}

/// Fetches `remote` into `owner`'s copy of it in the cache: written beside
/// it first and put in its place once whole, so a failed fetch leaves any
/// older copy as it was. With `keep_edits_since`, the copy's metadata when
/// it was last synced, a copy saved since then is left as it is, however
/// late the save comes (`swap_in`); without it the copy is replaced.
/// Blocking.
fn fetch_copy(
    io: &Io,
    owner: Owner,
    remote: &RemotePath,
    keep_edits_since: Option<watch::Meta>,
) -> Result<Fetch> {
    let cache = (io.cache)()?;
    let local = cache.copy_path(owner, &remote.target, &remote.path, remote.shell);
    if let Some(folder) = local.parent() {
        cache::create_private_dirs(folder)?;
    }
    let part = cache::part_path(&local);
    let fetched = write_part(io, remote, &part).inspect_err(|_| remove_part(&part))?;
    // Checked as late as possible: a save made while the fetch ran is kept.
    if let Some(synced) = keep_edits_since
        && edited_since(&local, synced).inspect_err(|_| remove_part(&part))?
    {
        remove_part(&part);
        return Ok(Fetch::KeptEdits);
    }
    #[cfg(test)]
    (io.step)(Step::Checked, &local);
    // Read from the whole part, whose modification time and length the
    // rename keeps: a save made as the copy lands then differs from it.
    let synced_meta = std::fs::metadata(&part)
        .and_then(|metadata| watch::Meta::of(&metadata))
        .map_err(|source| Error::Cache {
            path: part.clone(),
            source,
        })
        .inspect_err(|_| remove_part(&part))?;
    let placed = match keep_edits_since {
        Some(synced) => swap_in(io, &part, &local, synced),
        None => replace(io, &part, &local),
    };
    if placed.inspect_err(|_| remove_part(&part))? == Placed::KeptEdits {
        return Ok(Fetch::KeptEdits);
    }
    let opened = crate::window::opened(&local).and_then(|path| url::Url::from_file_path(path).ok());
    Ok(Fetch::Written(Box::new(Done {
        key: (remote.target.clone(), remote.path.clone()),
        copy: CachedCopy {
            local,
            policy: Policy::default(),
            shell: remote.shell,
            state: SyncState::Synced,
            synced_meta,
            synced_sha256: fetched.sha256,
            watch: watch::Watch::default(),
        },
        cache,
        opened,
    })))
}

/// Whether the copy at `local` differs from its metadata `synced`. A copy
/// that is gone holds no edits to keep.
fn edited_since(local: &Path, synced: watch::Meta) -> Result<bool> {
    match std::fs::metadata(local).and_then(|metadata| watch::Meta::of(&metadata)) {
        Ok(meta) => Ok(meta != synced),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(Error::Cache {
            path: local.to_owned(),
            source,
        }),
    }
}

/// Whether a fetch put the host's bytes in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placed {
    Written,
    /// A save was found at the copy's place, and stays there.
    KeptEdits,
}

/// Renames the whole `part` over the copy at `local`, whatever it holds.
fn replace(io: &Io, part: &Path, local: &Path) -> Result<Placed> {
    #[cfg(not(test))]
    let _ = io;
    std::fs::rename(part, local).map_err(|source| cache_error(local, source))?;
    #[cfg(test)]
    (io.step)(Step::Placed, local);
    Ok(Placed::Written)
}

/// Puts the whole `part` at `local` without ever replacing a save there.
/// The copy moves aside first, the part is linked into the place it left,
/// which fails rather than replace a save that took the place meanwhile,
/// and the copy moved aside then comes back over the host's bytes unless
/// its metadata proves it is still the one synced at `synced`. An editor
/// holding the copy open without letting it be renamed fails the fetch.
fn swap_in(io: &Io, part: &Path, local: &Path, synced: watch::Meta) -> Result<Placed> {
    #[cfg(not(test))]
    let _ = io;
    let kept = cache::kept_path(local);
    let kept = match std::fs::rename(local, &kept) {
        Ok(()) => Some(kept),
        // No copy, so nothing to keep.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => return Err(cache_error(local, source)),
    };
    #[cfg(test)]
    (io.step)(Step::MovedAside, local);
    match place(part, local) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // A save took the place in the gap: it stays, and the host's
            // bytes are dropped.
            remove_part(part);
            if let Some(kept) = &kept {
                discard_if_synced(kept, synced);
            }
            return Ok(Placed::KeptEdits);
        }
        Err(source) => {
            if let Some(kept) = &kept {
                put_back(kept, local, synced);
            }
            return Err(cache_error(local, source));
        }
    }
    #[cfg(test)]
    (io.step)(Step::Placed, local);
    remove_part(part);
    let Some(kept) = kept else {
        return Ok(Placed::Written);
    };
    match meta_of(&kept) {
        Ok(meta) if meta == synced => {
            remove_kept(&kept);
            return Ok(Placed::Written);
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(%error, kept = %kept.display(), "Remote file copy moved aside not checked; keeping it");
        }
    }
    // Saved after the check, or not proven unsaved: the save wins.
    std::fs::rename(&kept, local).map_err(|source| {
        tracing::warn!(kept = %kept.display(), "A save to a remote file copy was left beside it");
        cache_error(local, source)
    })?;
    Ok(Placed::KeptEdits)
}

/// Links `part` in at `local`, failing with `AlreadyExists` rather than
/// replace a file there. A filesystem without hard links gets a rename
/// instead, which replaces a save made in the moment before it; the copy
/// moved aside is still checked afterwards.
fn place(part: &Path, local: &Path) -> std::io::Result<()> {
    match std::fs::hard_link(part, local) {
        Err(error) if error.kind() == std::io::ErrorKind::Unsupported => {
            tracing::debug!(%error, copy = %local.display(), "No hard links in the remote file cache; renaming the copy into place");
            std::fs::rename(part, local)
        }
        linked => linked,
    }
}

/// Puts the copy moved aside to `kept` back at `local` after a failed
/// fetch. A save that took the place meanwhile stays there.
fn put_back(kept: &Path, local: &Path, synced: watch::Meta) {
    match place(kept, local) {
        Ok(()) => remove_kept(kept),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            discard_if_synced(kept, synced);
        }
        Err(error) => {
            tracing::warn!(%error, kept = %kept.display(), "Remote file copy not put back after a failed fetch");
        }
    }
}

/// Removes the copy moved aside to `kept` when its metadata proves it the
/// one synced at `synced`; one that may hold a save stays beside the copy.
fn discard_if_synced(kept: &Path, synced: watch::Meta) {
    match meta_of(kept) {
        Ok(meta) if meta == synced => remove_kept(kept),
        Ok(_) => {
            tracing::warn!(kept = %kept.display(), "A save to a remote file copy was left beside it")
        }
        Err(error) => {
            tracing::warn!(%error, kept = %kept.display(), "Remote file copy moved aside not checked; keeping it");
        }
    }
}

fn meta_of(path: &Path) -> std::io::Result<watch::Meta> {
    std::fs::metadata(path).and_then(|metadata| watch::Meta::of(&metadata))
}

fn cache_error(path: &Path, source: std::io::Error) -> Error {
    Error::Cache {
        path: path.to_owned(),
        source,
    }
}

fn remove_kept(kept: &Path) {
    if let Err(error) = std::fs::remove_file(kept)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, kept = %kept.display(), "Remote file copy moved aside not removed");
    }
}

fn write_part(io: &Io, remote: &RemotePath, part: &Path) -> Result<Fetched> {
    let mut file = cache::create_private_file(part)?;
    (io.fetch)(remote, &mut file).map_err(Error::Fetch)
}

fn remove_part(part: &Path) {
    if let Err(error) = std::fs::remove_file(part)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, part = %part.display(), "Partial remote file copy not removed");
    }
}

impl HerdrWindow {
    /// Fetches the host's file `remote` names into the cache and opens the
    /// copy with the system's default application, or says why it could not.
    /// A copy in conflict is reopened, and its question asked again.
    pub(crate) fn open_remote_file(&mut self, remote: RemotePath, cx: &mut Context<Self>) {
        if !self.remote_files.fetch_again(&remote) {
            let shown = self.shown_remote_file_prompt();
            let key = (remote.target.clone(), remote.path.clone());
            self.remote_files.reask_conflict(key, shown.as_ref());
            if let Some(copy) = self.remote_files.copy(&remote.target, &remote.path) {
                self.open_in_system_app(copy.local.clone(), cx);
            }
            return;
        }
        let key = (remote.target.clone(), remote.path.clone());
        // The fetch already running opens the copy when it is done.
        if !self.remote_files.fetching.insert(key.clone()) {
            return;
        }
        let name = basename(&remote.path, remote.shell);
        self.show_flash(
            Flash::success(format!("Fetching {name} from {}…", remote.target)),
            cx,
        );
        let io = self.remote_files.io.clone();
        let owner = self.remote_files.owner;
        let synced = self
            .remote_files
            .copy(&remote.target, &remote.path)
            .map(|copy| copy.synced_meta);
        let fetch = cx
            .background_executor()
            .spawn(async move { fetch_copy(&io, owner, &remote, synced) });
        cx.spawn(async move |this, cx| {
            let fetched = fetch.await;
            // The window may have closed meanwhile; nothing opens then.
            let _ = this.update(cx, |this, cx| this.remote_file_fetched(&key, fetched, cx));
        })
        .detach();
    }

    fn remote_file_fetched(&mut self, key: &Key, fetched: Result<Fetch>, cx: &mut Context<Self>) {
        self.remote_files.fetching.remove(key);
        match fetched {
            // Saved while the fetch ran: the copy with the save opens, and
            // the watch takes the save from there.
            Ok(Fetch::KeptEdits) => {
                if let Some(copy) = self.remote_files.copies.get(key) {
                    self.open_in_system_app(copy.local.clone(), cx);
                }
            }
            Ok(Fetch::Written(done)) => {
                let opened = done.opened.clone();
                self.remote_files.record(*done);
                match opened {
                    Some(url) => cx.open_url(url.as_str()),
                    None => tracing::warn!("Fetched remote file copy could not be opened"),
                }
            }
            Err(error) => {
                tracing::warn!(%error, "Remote file not fetched");
                self.show_flash(Flash::warning(crate::Error::from(error).to_string()), cx);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod tests;
