//! The helper process that applies a committed update: it revalidates the
//! stage, waits on the parent's commit barrier, swaps the installation, and
//! records the outcome for manual recovery.
use super::{
    HELPER, Mode, WAIT, authenticate, candidate, io,
    location::{detect, lock_file, no_links, owned},
    read_request,
};
use crate::error::{Error, Result};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, Write},
    os::unix::{
        ffi::OsStringExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    process::{ExitCode, Stdio},
    sync::{atomic::AtomicBool, mpsc},
    thread,
    time::{Duration, Instant},
};

pub(super) fn record_result(file: &mut File, result: &Result<()>) -> Result<()> {
    let text = match result {
        Ok(()) => "Update installed and restart process spawned. Startup health is not confirmed.\nKeep previous-installation until the new app is verified working.\n".to_owned(),
        Err(error) => {
            // Bound and escape even filenames/error text; no terminal controls
            // or shell commands from an error are copied into recovery guidance.
            let error: String = error.to_string().chars().flat_map(char::escape_default).take(4096).collect();
            format!("Update installation or restart failed.\nError: {error}\n\nRecovery:\nKeep this private staging directory, archive.tar.gz, and previous-installation (if present).\nClose all Sheprd instances; leave the daemon running.\nIf the original installation exists, try launching it manually; rollback may already have restored it.\nIf it is missing, restore previous-installation to the original installation location. Preserve any existing destination before replacing it; do not merge app bundles.\nIf there is no usable backup, manually install a verified signed release.\nNo automatic retry or cleanup will run.\n")
        }
    };
    file.rewind().map_err(io)?;
    file.write_all(text.as_bytes()).map_err(io)?;
    file.set_len(text.len() as u64).map_err(io)?;
    file.sync_all().map_err(io)
}

/// Where this update lands. A standalone `Herdr.app` given a `Sheprd.app` payload
/// moves to that name. Homebrew's receipt names the old path, so a bundle it
/// records stays put until its cask moves it, and an existing `Sheprd.app` is
/// never touched. `recorded` is asked only when a move is otherwise possible.
pub(super) fn target(
    destination: &Path,
    candidate: &Path,
    mode: Mode,
    recorded: impl FnOnce(&Path) -> bool,
) -> Result<PathBuf> {
    let renamed = super::bundle::renamed(destination)
        .filter(|renamed| mode == Mode::Mac && renamed.file_name() == candidate.file_name());
    let Some(renamed) = renamed else {
        return Ok(destination.to_owned());
    };
    if recorded(destination) {
        return Ok(destination.to_owned());
    }
    match fs::symlink_metadata(&renamed) {
        Ok(_) => Ok(destination.to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(renamed),
        Err(error) => Err(io(error)),
    }
}

/// Undo the legacy link, and only that: whatever else now sits at the path is
/// not ours to remove, so recovery stops there and both bundles survive.
fn remove_legacy_link(legacy: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(legacy) {
        Ok(meta) if meta.file_type().is_symlink() => fs::remove_file(legacy),
        Ok(_) => Err(std::io::ErrorKind::AlreadyExists.into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// `target` is `destination`, or the new name [`target`] chose for it.
pub(super) fn replace(
    destination: &Path,
    target: &Path,
    candidate: &Path,
    backup: &Path,
    mode: Mode,
    launch: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let moved = target != destination;
    if backup.try_exists().map_err(io)? {
        return Err(Error::RecoveryExists);
    }
    let original = fs::symlink_metadata(destination).map_err(io)?;
    match mode {
        Mode::Linux => fs::hard_link(destination, backup).map_err(io)?,
        Mode::Mac => fs::rename(destination, backup).map_err(io)?,
    }
    if let Err(error) = fs::rename(candidate, target) {
        if mode == Mode::Mac {
            if let Err(rollback) = fs::rename(backup, destination) {
                return Err(Error::ReplacementRollback {
                    source: error,
                    rollback,
                    backup: backup.to_owned(),
                });
            }
        } else if let Err(cleanup) = remove_failed_linux_backup(destination, backup, &original) {
            return Err(Error::ReplacementCleanup {
                source: error,
                cleanup: Box::new(cleanup),
                backup: backup.to_owned(),
            });
        }
        return Err(io(error));
    }
    let linked = if moved {
        super::bundle::link_legacy(destination)
    } else {
        Ok(())
    };
    if let Err(error) = linked.and_then(|()| launch()) {
        // Move aside only the exact candidate we just installed; never delete
        // an arbitrary installation tree. Both copies survive failed recovery.
        let unlinked = if moved {
            remove_legacy_link(destination)
        } else {
            Ok(())
        };
        if let Err(recovery) = unlinked
            .and_then(|()| fs::rename(target, candidate))
            .and_then(|()| fs::rename(backup, destination))
        {
            return Err(Error::RestartRecovery {
                source: Box::new(error),
                recovery,
                backup: backup.to_owned(),
            });
        }
        return Err(error);
    }
    File::open(
        destination
            .parent()
            .ok_or(Error::MissingDestinationParent)?,
    )
    .map_err(io)?
    .sync_all()
    .map_err(io)
}

pub(super) fn remove_failed_linux_backup(
    destination: &Path,
    backup: &Path,
    original: &fs::Metadata,
) -> Result<()> {
    let current = fs::symlink_metadata(destination).map_err(io)?;
    let saved = fs::symlink_metadata(backup).map_err(io)?;
    // Remove only our extra hardlink, never a changed destination, symlink,
    // or recovery file whose identity no longer matches the original inode.
    if !original.is_file()
        || [&current, &saved].iter().any(|meta| {
            !meta.is_file() || meta.dev() != original.dev() || meta.ino() != original.ino()
        })
    {
        return Err(Error::RecoveryChanged);
    }
    fs::remove_file(backup).map_err(io)
}

/// Pass arguments excluding argv[0], before initializing GPUI. Recognized but
/// malformed helper invocations fail closed rather than starting the GUI.
pub(crate) fn run_helper(args: &[OsString]) -> Option<ExitCode> {
    if args.first().is_none_or(|arg| arg != HELPER) {
        return None;
    }
    Some(if args.len() == 2 && helper(Path::new(&args[1])).is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn helper(stage: &Path) -> Result<()> {
    let cancel = AtomicBool::new(false);
    let installation = detect(&cancel)?;
    no_links(stage)?;
    if stage.parent() != installation.destination.parent()
        || !stage
            .file_name()
            .is_some_and(|name| name.as_encoded_bytes().starts_with(b".herdr-update-"))
    {
        return Err(Error::StageLocation);
    }
    if owned(stage, installation.uid, true)?.mode() & 0o077 != 0 {
        return Err(Error::StagePermissions);
    }
    let request = read_request(stage, installation.uid)?;
    let offer = authenticate(&request)?;
    let _handoff = lock_file(&installation, ".update-handoff")?;
    candidate(stage, &installation, &offer, &cancel)?;
    // Create once only after stage/request validation. Holding the descriptor
    // avoids following a replaced result path after the GUI has gone away.
    let mut report = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(stage.join("install-result.txt"))
        .map_err(io)?;
    report
        .write_all(b"Helper armed; no committed installation outcome recorded yet.\n")
        .map_err(io)?;
    report.sync_all().map_err(io)?;
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = std::io::stdin()
            .take(40)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(result);
    });
    std::io::stdout().write_all(b"READY\n").map_err(io)?;
    std::io::stdout().flush().map_err(io)?;
    let instruction = receiver
        .recv_timeout(WAIT)
        .map_err(Error::CommitBarrier)?
        .map_err(io)?;
    let mut expected = b"COMMIT\n".to_vec();
    expected.extend_from_slice(&request.token);
    if instruction != expected {
        return Err(Error::NotCommitted);
    }
    let result = (|| {
        let start = Instant::now();
        let _lease = loop {
            match lock_file(&installation, ".update-lock") {
                Ok(lease) => break lease,
                Err(Error::LockContended) if start.elapsed() < Duration::from_secs(5) => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(error) => return Err(error),
            }
        };
        let fresh = detect(&cancel)?;
        if fresh.destination != installation.destination {
            return Err(Error::InstallationChanged);
        }
        // Never install the previously inspected tree. Rebuild from the authenticated
        // archive after the parent closes the barrier, then validate again.
        let (tree, candidate) = candidate(stage, &installation, &offer, &cancel)?;
        let backup = stage.join("previous-installation");
        let args: Vec<OsString> = request.args.into_iter().map(OsString::from_vec).collect();
        let cwd = PathBuf::from(OsString::from_vec(request.cwd));
        let target = target(
            &installation.destination,
            &candidate,
            installation.mode,
            crate::brew::recorded,
        )?;
        let executable = replacement_executable(&target, &candidate, installation.mode)?;
        let result = replace(
            &installation.destination,
            &target,
            &candidate,
            &backup,
            installation.mode,
            || {
                sheprd_client::new_command(&executable)
                    .args(args)
                    .current_dir(cwd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(io)?;
                Ok(())
            },
        );
        // Retain failed candidate as well as old installation for manual recovery.
        let _ = tree.keep();
        result
    })();
    let recorded = record_result(&mut report, &result);
    match (result, recorded) {
        (Err(error), Err(record)) => Err(Error::RecordOutcome {
            source: Box::new(error),
            record: Box::new(record),
        }),
        (Err(error), _) => Err(error),
        (Ok(()), recorded) => recorded,
    }
}

/// Select from the validated candidate before moving it: the executable inside
/// the bundle can change during rebrand, whichever directory it lands in.
pub(super) fn replacement_executable(
    destination: &Path,
    candidate: &Path,
    mode: Mode,
) -> Result<PathBuf> {
    match mode {
        Mode::Linux => Ok(destination.to_owned()),
        Mode::Mac => {
            let executable = super::bundle::executable(candidate)?;
            let name = executable.file_name().ok_or(Error::BundleNotExecutable)?;
            Ok(destination.join("Contents/MacOS").join(name))
        }
    }
}
