//! Opening a folder printed in an SSH pane as a local copy: a probe of the
//! host's tree, the questions a large or changed tree asks, and the fetch,
//! extraction, and opening that follow. Every probe and transfer runs on the
//! background executor; the UI thread only starts them and applies results.

use super::{
    Error, FolderDone, Io, Key, MB, Owner, RemoteFiles, Result,
    archive::{self, ArchiveFormat},
    basename, cache, watch,
};
use crate::window::{Flash, HerdrWindow, RemotePath};
use gpui::Context;
use herdr_client::RemoteShell;
use std::{io, path::Path, sync::atomic::AtomicBool};

/// A question about a folder that waits for the window to have no page open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FolderPrompt {
    pub(crate) remote: RemotePath,
    pub(crate) kind: FolderQuestion,
    /// The probe this question was raised from; both answers that transfer
    /// reuse it, so an answer never probes the host again.
    pub(crate) probed: herdr_client::ProbedFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FolderQuestion {
    /// Its uncompressed contents exceed the configured warning size.
    TooLarge { bytes: u64 },
    /// A copy is here and its fingerprint differs from the host's.
    Changed { bytes: u64 },
}

impl RemoteFiles {
    /// Queues `prompt` unless it already waits.
    pub(crate) fn enqueue_folder(&mut self, prompt: FolderPrompt) {
        if !self.folder_prompts.contains(&prompt) {
            self.folder_prompts.push_back(prompt);
        }
    }

    /// The first waiting folder question, without removing it.
    pub(crate) fn next_folder_prompt(&self) -> Option<FolderPrompt> {
        self.folder_prompts.front().cloned()
    }

    /// Removes the question about `remote`, answered or dismissed.
    pub(crate) fn take_folder_prompt(&mut self, remote: &RemotePath) -> Option<FolderPrompt> {
        let index = self.folder_prompts.iter().position(|prompt| {
            prompt.remote.target == remote.target && prompt.remote.path == remote.path
        })?;
        self.folder_prompts.remove(index)
    }

    /// Ends the in-flight claim on `remote`'s copy when its question is
    /// dismissed, so a later click probes again.
    pub(crate) fn clear_folder_key(&mut self, remote: &RemotePath) {
        self.fetching
            .remove(&(remote.target.clone(), remote.path.clone()));
    }

    /// Forgets the copy of `key` and any question about it, as when the
    /// host's path stopped being the kind of thing the copy holds.
    pub(crate) fn forget(&mut self, key: &Key) {
        self.copies.remove(key);
        self.folder_prompts
            .retain(|prompt| prompt.remote.target != key.0 || prompt.remote.path != key.1);
    }
}

impl HerdrWindow {
    /// Probes the host folder `remote` names and opens the copy already here,
    /// or copies the folder and opens that.
    pub(crate) fn open_remote_folder(&mut self, remote: RemotePath, cx: &mut Context<Self>) {
        let key = (remote.target.clone(), remote.path.clone());
        // The running probe or transfer owns the copy until it ends or the
        // question it asked is dismissed.
        if !self.remote_files.fetching.insert(key.clone()) {
            return;
        }
        let name = basename(&remote.path, remote.shell).to_owned();
        self.show_flash(
            Flash::success(format!("Checking {name} on {}…", remote.target)),
            cx,
        );
        let io = self.remote_files.io.clone();
        let owner = self.remote_files.owner;
        let has_copy = self
            .remote_files
            .folder(&remote.target, &remote.path)
            .is_some();
        let task_remote = remote.clone();
        let probe = cx.background_executor().spawn(async move {
            // The UI thread asks no disk whether the copy is still there.
            let present = has_copy
                && (io.cache)().is_ok_and(|cache| {
                    cache
                        .copy_path(
                            owner,
                            &task_remote.target,
                            &task_remote.path,
                            task_remote.shell,
                        )
                        .is_dir()
                });
            (present, (io.folder_probe)(&task_remote))
        });
        cx.spawn(async move |this, cx| {
            let (present, probed) = probe.await;
            // The window may have closed meanwhile; nothing opens then.
            let _ = this.update(cx, |this, cx| {
                this.remote_folder_probed(key, remote, present, probed, cx)
            });
        })
        .detach();
    }

    /// Applies a probe of `remote`'s folder: opens the copy that still
    /// matches, asks about a changed or large one, or copies it.
    fn remote_folder_probed(
        &mut self,
        key: Key,
        remote: RemotePath,
        present: bool,
        probed: herdr_client::Result<herdr_client::ProbedFolder>,
        cx: &mut Context<Self>,
    ) {
        let probed = match probed {
            Ok(probed) => probed,
            Err(error) => {
                self.remote_files.fetching.remove(&key);
                // The host's path is a file now: a later click fetches it.
                if matches!(
                    &error,
                    herdr_client::Error::RemoteFile(herdr_client::RemoteFileError::NotAFolder)
                ) {
                    self.remote_files.forget(&key);
                }
                self.show_flash(
                    Flash::warning(crate::Error::from(Error::Probe(error)).to_string()),
                    cx,
                );
                return;
            }
        };
        if let Some((local, fingerprint)) = self
            .remote_files
            .folder(&remote.target, &remote.path)
            .filter(|_| present)
        {
            // Both fingerprints prove the host's tree unchanged; a host
            // without one is copied again rather than guessed at.
            if fingerprint.is_some() && fingerprint == probed.fingerprint {
                self.remote_files.fetching.remove(&key);
                self.open_in_system_app(local, None, cx);
            } else {
                self.remote_files.enqueue_folder(FolderPrompt {
                    remote,
                    kind: FolderQuestion::Changed {
                        bytes: probed.bytes,
                    },
                    probed,
                });
            }
            return;
        }
        let warn = u64::from(self.config.remote_files.folder_warning_mb) * MB;
        if probed.bytes > warn {
            self.remote_files.enqueue_folder(FolderPrompt {
                remote,
                kind: FolderQuestion::TooLarge {
                    bytes: probed.bytes,
                },
                probed,
            });
            return;
        }
        self.start_folder_transfer(remote, probed, cx);
    }

    /// Copies `remote`'s folder, which `probed` sized, and opens the copy.
    pub(crate) fn start_folder_transfer(
        &mut self,
        remote: RemotePath,
        probed: herdr_client::ProbedFolder,
        cx: &mut Context<Self>,
    ) {
        let name = basename(&remote.path, remote.shell).to_owned();
        let cap = u64::from(self.config.remote_files.folder_transfer_cap_mb) * MB;
        self.show_flash(
            Flash::success(format!(
                "Fetching {name} ({}) from {}…",
                megabytes(probed.bytes),
                remote.target
            )),
            cx,
        );
        let io = self.remote_files.io.clone();
        let owner = self.remote_files.owner;
        let key = (remote.target.clone(), remote.path.clone());
        let fetch = cx
            .background_executor()
            .spawn(async move { fetch_folder(&io, owner, &remote, probed, cap) });
        cx.spawn(async move |this, cx| {
            let fetched = fetch.await;
            // The window may have closed meanwhile; nothing opens then.
            let _ = this.update(cx, |this, cx| this.remote_folder_fetched(key, fetched, cx));
        })
        .detach();
    }

    /// Applies a folder copy: keeps it, opens it, and says what it skipped.
    fn remote_folder_fetched(
        &mut self,
        key: Key,
        fetched: Result<FolderFetched>,
        cx: &mut Context<Self>,
    ) {
        self.remote_files.fetching.remove(&key);
        match fetched {
            Ok(fetched) => {
                let local = fetched.done.local.clone();
                let skipped = fetched.links_skipped;
                self.remote_files.record_folder(fetched.done);
                self.open_in_system_app(local, None, cx);
                if skipped > 0 {
                    let text = if skipped == 1 {
                        "1 link was skipped".to_owned()
                    } else {
                        format!("{skipped} links were skipped")
                    };
                    self.show_flash(Flash::success(text), cx);
                }
            }
            Err(error) => {
                tracing::warn!(%error, "Remote folder not copied");
                self.show_flash(Flash::warning(crate::Error::from(error).to_string()), cx);
            }
        }
    }
}

/// A finished folder fetch, ready to record.
struct FolderFetched {
    done: FolderDone,
    links_skipped: u32,
}

/// What one transfer left at the copy's place.
struct Filled {
    meta: watch::Meta,
    links_skipped: u32,
}

/// Where one folder transfer writes, and the size limit it fetches under.
struct Transfer<'a> {
    archive: &'a Path,
    staging: &'a Path,
    local: &'a Path,
    cap_bytes: u64,
}

/// Fetches `remote`'s folder as an archive and extracts it at `owner`'s copy
/// of the path, replacing any copy there. Blocking: call only on a
/// background worker.
fn fetch_folder(
    io: &Io,
    owner: Owner,
    remote: &RemotePath,
    probed: herdr_client::ProbedFolder,
    cap_bytes: u64,
) -> Result<FolderFetched> {
    let cache = (io.cache)()?;
    let local = cache.copy_path(owner, &remote.target, &remote.path, remote.shell);
    let parent = local.parent().ok_or_else(|| Error::Cache {
        path: local.clone(),
        source: io::Error::new(io::ErrorKind::InvalidInput, "the copy has no folder"),
    })?;
    cache::create_private_dirs(parent)?;
    // Both calls name a path no other transfer of this process uses.
    let archive = cache::part_path(&local);
    let staging = cache::part_path(&local);
    cache::create_private_dirs(&staging)?;
    let cancelled = AtomicBool::new(false);
    let transfer = Transfer {
        archive: &archive,
        staging: &staging,
        local: &local,
        cap_bytes,
    };
    let filled = fill(io, remote, &probed.top, transfer, &cancelled);
    let filled = match filled {
        Ok(filled) => filled,
        Err(error) => {
            remove_file(&archive);
            remove_dir(&staging);
            return Err(error);
        }
    };
    Ok(FolderFetched {
        done: FolderDone {
            key: (remote.target.clone(), remote.path.clone()),
            cache,
            local,
            shell: remote.shell,
            meta: filled.meta,
            bytes: probed.bytes,
            fingerprint: probed.fingerprint,
        },
        links_skipped: filled.links_skipped.saturating_add(probed.links_skipped),
    })
}

/// Fetches, extracts, and moves the folder into place. `top` is the name the
/// host's archive entries start with. Blocking.
fn fill(
    io: &Io,
    remote: &RemotePath,
    top: &str,
    transfer: Transfer<'_>,
    cancelled: &AtomicBool,
) -> Result<Filled> {
    let mut file = archive::create_file(transfer.archive).map_err(|source| Error::Archive {
        path: transfer.archive.to_owned(),
        source,
    })?;
    let fetched =
        (io.folder_fetch)(remote, &mut file, transfer.cap_bytes).map_err(|error| match error {
            herdr_client::Error::RemoteFile(herdr_client::RemoteFileError::TooLarge { limit }) => {
                Error::FolderTooLarge { limit }
            }
            other => Error::Fetch(other),
        })?;
    drop(file);
    let extracted = archive::extract(
        transfer.archive,
        transfer.staging,
        top,
        format_for(remote.shell),
        cancelled,
    )?;
    // The archive is no use once unpacked; failing to remove it is not.
    remove_file(transfer.archive);
    if transfer.local.is_dir() {
        std::fs::remove_dir_all(transfer.local).map_err(|source| Error::FolderInUse {
            path: transfer.local.to_owned(),
            source,
        })?;
    } else if transfer.local.exists() {
        // A file copy of the same path, before the host's path became a folder.
        std::fs::remove_file(transfer.local).map_err(|source| Error::FolderInUse {
            path: transfer.local.to_owned(),
            source,
        })?;
    }
    std::fs::rename(transfer.staging, transfer.local).map_err(|source| Error::Cache {
        path: transfer.local.to_owned(),
        source,
    })?;
    let meta = std::fs::metadata(transfer.local)
        .and_then(|meta| watch::Meta::of(&meta))
        .map_err(|source| Error::Cache {
            path: transfer.local.to_owned(),
            source,
        })?;
    tracing::debug!(bytes = fetched.bytes, "Remote folder archive fetched");
    Ok(Filled {
        meta,
        links_skipped: extracted.links_skipped,
    })
}

/// The archive format a host's shell sends a folder in.
fn format_for(shell: RemoteShell) -> ArchiveFormat {
    match shell {
        RemoteShell::Posix => ArchiveFormat::TarGz,
        RemoteShell::PowerShell => ArchiveFormat::Zip,
    }
}

/// `bytes` as megabytes with one decimal.
fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / MB as f64)
}

fn remove_file(path: &Path) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(%error, path = %path.display(), "Remote folder archive not removed");
    }
}

fn remove_dir(path: &Path) {
    if let Err(error) = std::fs::remove_dir_all(path)
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(%error, path = %path.display(), "Remote folder staging not removed");
    }
}
