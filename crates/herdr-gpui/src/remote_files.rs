//! Files on SSH hosts that a link-modifier click opens here. The click
//! fetches the host's file over the endpoint's SSH trust into a private
//! cache folder, then opens the copy with the system's default application
//! under the same rule as a local path: a document itself, and anything
//! else the folder holding it. The window records each copy it made, keyed
//! by host and path, and the copies are removed when the app quits.
//!
//! Every fetch and every disk access runs on the background executor; the
//! UI thread only starts a fetch and applies its result.

mod cache;

use crate::window::{Flash, HerdrWindow, RemotePath};
use gpui::{App, Context, Task};
use herdr_client::{Fetched, MAX_REMOTE_FILE_BYTES, RemoteShell};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::SystemTime,
};

pub(crate) use cache::Cache;

/// Why a remote file was not fetched or opened.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not use the remote file cache at {}: {source}", path.display())]
    Cache {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Could not fetch the file: {0}")]
    Fetch(#[source] herdr_client::Error),
    #[error("No state folder for the remote file cache: set XDG_STATE_HOME, HOME, or LOCALAPPDATA")]
    NoStateDir,
    // keep-sorted end
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

/// Streams the host's file `RemotePath` names into the writer. Blocking.
pub(crate) type Fetcher = Arc<
    dyn Fn(&RemotePath, &mut (dyn Write + Send)) -> herdr_client::Result<Fetched> + Send + Sync,
>;

/// The cache copies are written to. Blocking.
pub(crate) type CacheSource = Arc<dyn Fn() -> Result<Arc<Cache>> + Send + Sync>;

/// The I/O a fetch does: SSH and the process's cache in the app, and fakes
/// in tests.
#[derive(Clone)]
pub(crate) struct Io {
    cache: CacheSource,
    fetch: Fetcher,
}

impl Default for Io {
    fn default() -> Self {
        Self {
            cache: Arc::new(cache::shared),
            fetch: Arc::new(|remote, out| {
                // The script's idle timeout ends a transfer that stalls.
                herdr_client::fetch_remote_file(
                    &remote.target,
                    remote.shell,
                    &remote.path,
                    out,
                    MAX_REMOTE_FILE_BYTES,
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

/// A local copy of a host's file, as it was right after it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "what the copy was fetched as, to tell local edits from the host's bytes; only `local` is read so far"
)]
pub(crate) struct CachedCopy {
    pub(crate) local: PathBuf,
    pub(crate) sha256: [u8; 32],
    pub(crate) shell: RemoteShell,
    pub(crate) modified: SystemTime,
    pub(crate) len: u64,
}

/// The copies one window fetched, by SSH target and host path.
#[derive(Debug, Default)]
pub(crate) struct RemoteFiles {
    /// The cache the copies are in, once one was written.
    cache: Option<Arc<Cache>>,
    copies: HashMap<(String, String), CachedCopy>,
    io: Io,
}

impl RemoteFiles {
    /// The copy last fetched of `path` on `target`.
    pub(crate) fn copy(&self, target: &str, path: &str) -> Option<&CachedCopy> {
        self.copies.get(&(target.to_owned(), path.to_owned()))
    }

    /// Whether a click on `remote` fetches it again rather than reopening
    /// the copy already here. It always does: the host holds the file, and
    /// its bytes may be newer than the copy's.
    pub(crate) fn fetch_again(&self, _remote: &RemotePath) -> bool {
        true
    }

    fn record(&mut self, done: Done) {
        self.cache = Some(done.cache);
        self.copies.insert(done.key, done.copy);
    }

    /// Forgets the copies and removes the cache folder holding them, which
    /// every window of the process shares, off the UI thread.
    pub(crate) fn remove_copies(&mut self, cx: &App) -> Task<()> {
        self.copies.clear();
        let cache = self.cache.take();
        cx.background_executor().spawn(async move {
            if let Some(cache) = cache
                && let Err(error) = cache.remove()
            {
                tracing::warn!(%error, "Remote file copies not removed");
            }
        })
    }
}

/// A finished fetch: the copy written, and what a click opens for it.
struct Done {
    key: (String, String),
    copy: CachedCopy,
    cache: Arc<Cache>,
    /// The copy, or the folder holding it when it is not a document.
    opened: Option<url::Url>,
}

/// Fetches `remote` into its copy in the cache: written beside it first and
/// renamed over it once whole, so a failed fetch leaves any older copy as
/// it was. Blocking.
fn fetch_copy(io: &Io, remote: &RemotePath) -> Result<Done> {
    let cache = (io.cache)()?;
    let local = cache.copy_path(&remote.target, &remote.path);
    if let Some(folder) = local.parent() {
        cache::create_private_dirs(folder)?;
    }
    let mut name = local.file_name().unwrap_or_default().to_owned();
    name.push(".part");
    let part = local.with_file_name(name);
    let fetched = write_part(io, remote, &part)
        .and_then(|fetched| {
            std::fs::rename(&part, &local)
                .map(|()| fetched)
                .map_err(|source| Error::Cache {
                    path: local.clone(),
                    source,
                })
        })
        .inspect_err(|_| remove_part(&part))?;
    let metadata =
        std::fs::metadata(&local).and_then(|metadata| Ok((metadata.modified()?, metadata.len())));
    let (modified, len) = metadata.map_err(|source| Error::Cache {
        path: local.clone(),
        source,
    })?;
    let opened = crate::window::opened(&local).and_then(|path| url::Url::from_file_path(path).ok());
    Ok(Done {
        key: (remote.target.clone(), remote.path.clone()),
        copy: CachedCopy {
            local,
            sha256: fetched.sha256,
            shell: remote.shell,
            modified,
            len,
        },
        cache,
        opened,
    })
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
    pub(crate) fn open_remote_file(&mut self, remote: RemotePath, cx: &mut Context<Self>) {
        if !self.remote_files.fetch_again(&remote) {
            if let Some(copy) = self.remote_files.copy(&remote.target, &remote.path) {
                self.open_in_system_app(copy.local.clone(), cx);
            }
            return;
        }
        let name = cache::basename(&remote.path);
        self.show_flash(
            Flash::success(format!("Fetching {name} from {}…", remote.target)),
            cx,
        );
        let io = self.remote_files.io.clone();
        let fetch = cx
            .background_executor()
            .spawn(async move { fetch_copy(&io, &remote) });
        cx.spawn(async move |this, cx| {
            let fetched = fetch.await;
            // The window may have closed meanwhile; nothing opens then.
            let _ = this.update(cx, |this, cx| this.remote_file_fetched(fetched, cx));
        })
        .detach();
    }

    fn remote_file_fetched(&mut self, fetched: Result<Done>, cx: &mut Context<Self>) {
        match fetched {
            Ok(done) => {
                let opened = done.opened.clone();
                self.remote_files.record(done);
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
