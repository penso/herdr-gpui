//! The accepted bundle layout is independent of its installed directory name:
//! an in-place update keeps that directory (and Dock/Homebrew references) stable.
use crate::{Error, Result};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub(crate) const ROOTS: &[&str] = &["Herdr.app", "Sheprd.app"];
const EXECUTABLES: &[&str] = &["Herdr", "sheprd"];

pub(super) fn accepts_executable(name: &str) -> bool {
    EXECUTABLES.contains(&name)
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
