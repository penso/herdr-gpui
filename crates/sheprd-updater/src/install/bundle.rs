//! The accepted bundle layout is independent of its installed directory name.
//! An update keeps that directory, except that a standalone `Herdr.app` given a
//! `Sheprd.app` payload moves to the new name and leaves a hidden link behind.
use crate::{Error, Result};
use std::{
    fs,
    os::unix::fs::{MetadataExt, symlink},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

const LEGACY: &str = "Herdr.app";
const CURRENT: &str = "Sheprd.app";
pub(crate) const ROOTS: &[&str] = &[LEGACY, CURRENT];
const EXECUTABLES: &[&str] = &["Herdr", "sheprd"];

pub(super) fn accepts_executable(name: &str) -> bool {
    EXECUTABLES.contains(&name)
}

/// Where a bundle at the legacy name belongs since the rename: beside itself,
/// under the current name. `None` for any other name.
pub(crate) fn renamed(bundle: &Path) -> Option<PathBuf> {
    (bundle.file_name() == Some(LEGACY.as_ref())).then(|| bundle.with_file_name(CURRENT))
}

/// Point the legacy path at the renamed bundle beside it. A core started from
/// the old bundle outlives the app, and LaunchAgents and scripts keep its path,
/// so that path must go on resolving. Relative, so it survives moving the
/// folder. Finder hides it; a failure to hide costs only tidiness.
pub(crate) fn link_legacy(legacy: &Path) -> Result<()> {
    symlink(CURRENT, legacy).map_err(Error::Io)?;
    let mut command = sheprd_client::new_command("/usr/bin/chflags");
    command.args(["-h", "hidden"]).arg(legacy);
    if let Err(error) = super::output(&mut command, &AtomicBool::new(false)) {
        tracing::warn!(category = "updater", %error, "could not hide the legacy app link");
    }
    Ok(())
}

/// Require exactly one regular, executable entry. Never follow a symlink or
/// guess which of two binaries the bundle meant to launch. Code-sign validation
/// also checks CFBundleExecutable against this choice before installation.
pub(crate) fn executable(bundle: &Path) -> Result<PathBuf> {
    let mut found = None;
    for name in EXECUTABLES {
        let path = bundle.join("Contents/MacOS").join(name);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() && meta.mode() & 0o111 != 0 => {
                if found.replace(path).is_some() {
                    return Err(Error::BundleNotExecutable);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            _ => return Err(Error::BundleNotExecutable),
        }
    }
    found.ok_or(Error::BundleNotExecutable)
}
