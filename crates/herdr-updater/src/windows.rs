//! Windows updates through the release's per-user `-setup.exe`.
//!
//! The installer is trusted only after two independent checks of the bytes
//! this process holds open: their SHA-256 matches the release's Ed25519-signed
//! `windows-manifest.json`, and Windows verifies their Authenticode signature
//! to a trusted root, made by the publisher this build names in
//! `HERDR_WINDOWS_SIGNER`. Both run again right before the installer starts,
//! and the file stays open for reading only, with no sharing for writes or
//! deletion, from the first check until this process exits.
//!
//! On commit the app starts the installer silently and quits. Setup closes any
//! copy still running through Restart Manager, replaces the files, and starts
//! the app again (its `/RELAUNCH=1` entry in `herdr-gpui.iss`). Herdr sessions
//! live in the daemon and keep running throughout.
//!
//! winget installs this same installer for the current user and reads the
//! version back from the uninstall entry Setup rewrites, so an in-app update
//! leaves `winget list` and `winget upgrade` in agreement; there is no
//! separate winget-managed layout to defer to.
mod authenticode;

use super::{error::Result, release};
use std::{
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Seek, SeekFrom},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{ExitCode, Stdio},
    sync::atomic::{AtomicBool, Ordering},
};
use tempfile::TempDir;
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

/// Where `herdr-gpui.iss` and `site/install.ps1` both install, under
/// `%LOCALAPPDATA%`.
const INSTALL_DIR: [&str; 2] = ["Programs", "Herdr UI"];
const EXECUTABLE: &str = "herdr-gpui.exe";
const STAGE_PREFIX: &str = "herdr-update-";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "In-app updates replace the copy installed in %LOCALAPPDATA%\\Programs\\Herdr UI, and this one runs from elsewhere. Download the new release from GitHub instead"
    )]
    Location,
    #[error("%LOCALAPPDATA% is not set, so the installed copy cannot be found")]
    LocalAppData,
    #[error(
        "This build names no Windows publisher to check update installers against. Download the new release from GitHub instead"
    )]
    MissingPublisher,
    #[error("The update installer's path cannot be passed to Windows")]
    InstallerPath,
    #[error("Windows does not trust the update installer's signature (0x{0:08x})")]
    Untrusted(i32),
    #[error("Windows verified the update installer but named no signer")]
    MissingSigner,
    #[error("The update installer is signed by {found:?}, not by Herdr UI's publisher")]
    Signer { found: String },
    #[error("Could not start the update installer")]
    Launch(#[source] io::Error),
}

/// The publisher every release installer is signed by, fixed at build time
/// from the release workflow's `WINDOWS_SIGNING_SUBJECT`.
fn publisher() -> Option<&'static str> {
    option_env!("HERDR_WINDOWS_SIGNER").filter(|subject| !subject.is_empty())
}

/// Why this build can never update itself, if it cannot. Reads nothing from
/// disk, so the UI thread may ask.
pub(crate) fn missing_publisher() -> Option<Error> {
    publisher().is_none().then_some(Error::MissingPublisher)
}

/// Why this copy cannot update itself, if it cannot. Resolves paths, so only
/// the update worker asks, before every check, download, and install: the
/// user may move or replace the installation.
pub(crate) fn unsupported() -> Option<Error> {
    missing_publisher().or_else(|| installed_here().err())
}

fn installed_here() -> std::result::Result<(), Error> {
    let local = env::var_os("LOCALAPPDATA").ok_or(Error::LocalAppData)?;
    let expected = INSTALL_DIR
        .iter()
        .fold(PathBuf::from(local), |path, part| path.join(part))
        .join(EXECUTABLE);
    let running = env::current_exe().map_err(|_| Error::Location)?;
    same_file(&running, &expected)
        .then_some(())
        .ok_or(Error::Location)
}

/// Windows paths compare without case, and either may be a link or 8.3 name.
fn same_file(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// A verified installer, held open in a private staging directory.
pub(crate) struct Prepared {
    // Declared before `stage` so it closes first: the held file shares no
    // deletion, and the stage's removal on drop would fail around it.
    file: File,
    stage: TempDir,
    installer: PathBuf,
    asset: release::Asset,
}

pub(crate) fn prepare(
    offer: &release::Offer,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<Prepared> {
    if let Some(error) = unsupported() {
        return Err(error.into());
    }
    remove_stale_stages();
    let stage = tempfile::Builder::new()
        .prefix(STAGE_PREFIX)
        .tempdir()
        .map_err(crate::Error::Io)?;
    // The name is one the signed manifest allowed: `Herdr-VERSION-TARGET-setup.exe`.
    let installer = stage.path().join(&offer.asset.name);
    release::download(offer, &installer, cancel, progress)?;
    let file = hold(&installer)?;
    let prepared = Prepared {
        file,
        stage,
        installer,
        asset: offer.asset.clone(),
    };
    prepared.verify(publisher().ok_or(Error::MissingPublisher)?, cancel)?;
    Ok(prepared)
}

/// Opens the downloaded installer so that nothing can change or delete it
/// while this process holds it, though Windows can still run it.
fn hold(path: &Path) -> Result<File> {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .map_err(crate::Error::Io)
}

impl Prepared {
    /// The held bytes match the signed manifest, then Windows trusts their
    /// signature and `publisher` made it.
    fn verify(&self, publisher: &str, cancel: &AtomicBool) -> Result<()> {
        let mut file = &self.file;
        file.seek(SeekFrom::Start(0)).map_err(crate::Error::Io)?;
        release::copy_archive(file, io::sink(), &self.asset, cancel, |_, _| {})?;
        authenticode::verify(&self.file, &self.installer, publisher)?;
        Ok(())
    }
}

/// Stages from earlier updates stay behind once their installer ran, with its
/// log; clear the ones older than a day. Only this updater's own prefix.
fn remove_stale_stages() {
    let Ok(entries) = fs::read_dir(env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age.as_secs() > 24 * 60 * 60);
        if old
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with(STAGE_PREFIX)
            && entry.file_type().is_ok_and(|kind| kind.is_dir())
        {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Starts the installer when the app commits to quitting. Dropping it
/// uncommitted deletes the download.
pub(crate) struct RestartGuard {
    prepared: Option<Prepared>,
}

impl RestartGuard {
    /// Call only once the UI has decided to quit. Starting one process is the
    /// only work here; every check already ran on the worker.
    pub(crate) fn commit(&mut self) -> Result<()> {
        let Some(prepared) = self.prepared.take() else {
            return Ok(());
        };
        let log = prepared.stage.path().join("install.log");
        herdr_client::new_command(&prepared.installer)
            .args(installer_args(&log))
            .current_dir(prepared.stage.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(Error::Launch)?;
        // Setup runs from the stage after this process exits, and writes its
        // log there; a later update clears it.
        let _ = prepared.stage.keep();
        Ok(())
    }
}

/// Silent, without restarting Windows, closing a running copy through Restart
/// Manager, and starting the app again once installed.
fn installer_args(log: &Path) -> Vec<OsString> {
    let mut log_arg = OsString::from("/LOG=");
    log_arg.push(log);
    [
        "/VERYSILENT",
        "/SUPPRESSMSGBOXES",
        "/NORESTART",
        "/CLOSEAPPLICATIONS",
        "/RELAUNCH=1",
    ]
    .into_iter()
    .map(OsString::from)
    .chain([log_arg])
    .collect()
}

/// Rechecks the held installer right before the UI may commit.
pub(crate) fn install_and_restart(prepared: Prepared, cancel: &AtomicBool) -> Result<RestartGuard> {
    if cancel.load(Ordering::Relaxed) {
        return Err(crate::Error::Cancelled);
    }
    if let Some(error) = unsupported() {
        return Err(error.into());
    }
    prepared.verify(publisher().ok_or(Error::MissingPublisher)?, cancel)?;
    Ok(RestartGuard {
        prepared: Some(prepared),
    })
}

/// Homebrew bundles are macOS-only; the cask lookup never gets this far.
pub(crate) fn effective_uid() -> Result<u32> {
    Err(crate::Error::UnsupportedTarget)
}

pub(crate) fn mac_bundle(_executable: &Path) -> Result<PathBuf> {
    Err(crate::Error::NotHerdrBundle)
}

/// Setup itself replaces the files, so no helper mode exists here.
pub(crate) fn run_helper(_args: &[OsString]) -> Option<ExitCode> {
    None
}

#[cfg(test)]
mod tests;
