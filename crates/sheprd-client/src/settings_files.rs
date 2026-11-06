//! The files behind Sheprd's settings, as the app and
//! `herdr-remote-control enroll` both handle them: `config-gpui.toml`, which
//! is managed unless it predates overrides, `config-gpui.local.toml`, the
//! user's overrides, and the lock that keeps their writers apart.
use std::{
    fs,
    io::{self, ErrorKind, Write},
    path::{Path, PathBuf},
};

/// The first line of Sheprd's managed `herdr/config-gpui.toml`, which it
/// rewrites at every launch and never reads. A file without it is a
/// personal config from before `config-gpui.local.toml`, which Sheprd moves
/// into that file.
pub const MANAGED_HEADER: &str = "# DO NOT EDIT -- WILL BE OVERWRITTEN";

/// What Sheprd's first launch writes to `herdr/config-gpui.local.toml`.
/// Existing overrides and moved personal configs are never rewritten, so
/// settings placed here reach new installs without changing what current
/// users see.
// i18n: data
pub const OVERRIDES_SEED: &str = "# Sheprd overrides. Saved changes reload automatically.\n# Unset keys inherit config-gpui.toml; tables merge key by key.\n\n# New installs start with the roomy rounded sidebar. Remove this line for\n# the managed default, or pick another layout listed in config-gpui.toml.\nlayout = \"comfortable-rounded\"\n";

/// Where a host's settings stand, from a personal `config-gpui.toml` and
/// the overrides beside it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    /// Neither exists: first launch seeds the overrides.
    Fresh,
    /// Only a personal file: Sheprd moves it into the overrides.
    Personal,
    /// Only overrides, the state of a current install.
    Overrides,
    /// A byte-identical copy of the personal file: a move Sheprd began and
    /// finishes next.
    Moving,
    /// Both exist and differ, which Sheprd leaves for a person to settle.
    Conflict,
}

impl State {
    pub fn of(personal: Option<&str>, overrides: Option<&str>) -> Self {
        match (personal, overrides) {
            (None, None) => Self::Fresh,
            (Some(_), None) => Self::Personal,
            (None, Some(_)) => Self::Overrides,
            (Some(personal), Some(overrides)) if personal == overrides => Self::Moving,
            (Some(_), Some(_)) => Self::Conflict,
        }
    }
}

/// `text` when it is a personal `config-gpui.toml` rather than the managed
/// one. Compares the first line so Windows checkouts and editors can use
/// CRLF.
pub fn personal(text: &str) -> Option<&str> {
    (text.lines().next() != Some(MANAGED_HEADER)).then_some(text)
}

/// The overrides beside the settings at `path`.
pub fn overrides_path(path: &Path) -> PathBuf {
    path.with_extension("local.toml")
}

/// The lock beside the settings at `path`.
pub fn lock_path(path: &Path) -> PathBuf {
    path.with_extension("lock")
}

/// Opens the lock file at `path`, unlocked, creating it when missing.
pub fn open_lock(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

/// A settings file's text, `None` when there is none. A link to nothing
/// reads as none too; [`create`] refuses to replace it.
pub fn read(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Creates the file at `path`, failing when anything is there, even a link
/// to nothing: that may be a checkout not mounted yet.
pub fn create(path: &Path, text: &str) -> io::Result<()> {
    temporary(parent(path), text)?
        .persist_noclobber(path)
        .map_err(|error| error.error)?;
    Ok(())
}

/// Replaces the file at `path` whole, so a write cut short never leaves
/// half of one.
pub fn replace(path: &Path, text: &str) -> io::Result<()> {
    // rename(2) replaces a symlink instead of writing through it, so write
    // to the file the link points at. A missing path keeps its own name.
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let parent = parent(&target);
    fs::create_dir_all(parent)?;
    temporary(parent, text)?
        .persist(&target)
        .map_err(|error| error.error)?;
    Ok(())
}

fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

/// A private file in `dir` holding `text`, on disk.
fn temporary(dir: &Path, text: &str) -> io::Result<tempfile::NamedTempFile> {
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(text.as_bytes())?;
    file.as_file().sync_all()?;
    Ok(file)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
