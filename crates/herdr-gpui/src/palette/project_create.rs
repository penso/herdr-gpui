//! Creating a new project inside a configured palette root, and opening it.
//!
//! The palette already opens an *existing* directory as a workspace
//! (`palette::project_open`). This supplies the missing verb: making the
//! directory. Nothing here invents a location — the new project goes inside a
//! `palette.project_roots` entry the user configured, which is the same
//! convention project discovery already uses.

use super::{Action, Entry, Palette, projects};
use crate::{Error, pull_request::run};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

/// Generous: a large repository over a slow link is not an error.
const CLONE_TIMEOUT: Duration = Duration::from_secs(900);

/// What fills the new directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Source {
    /// An empty folder.
    Folder,
    /// `git clone <url>`.
    Clone { url: String },
}

/// A project the palette can create: where it goes, what it is called, and how
/// it is filled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NewProject {
    pub(super) path: PathBuf,
    pub(super) label: String,
    pub(super) source: Source,
}

/// One path segment that is safe to use as a folder name.
///
/// Deliberately narrow: a leading dot (hidden), a separator, or a colon (a
/// Windows drive or an scp-style remote) is refused rather than guessed at.
pub(super) fn parse_name(text: &str) -> Option<String> {
    let name = text.trim();
    if name.is_empty() || name.len() > 128 || name == "." || name == ".." {
        return None;
    }
    if name.starts_with('.') {
        return None;
    }
    if name
        .chars()
        .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | ':'))
    {
        return None;
    }
    Some(name.to_owned())
}

/// Recognise a repository reference, returning `(url, name)`.
///
/// Only explicit URLs: a bare `owner/repo` is refused because it would have to
/// pick a forge on the user's behalf.
pub(super) fn parse_repo(text: &str) -> Option<(String, String)> {
    let raw = text.trim();
    let explicit = ["https://", "http://", "ssh://", "git://"]
        .iter()
        .any(|prefix| raw.starts_with(prefix))
        || raw.starts_with("git@");
    if !explicit {
        return None;
    }
    let tail = raw.trim_end_matches('/').rsplit(['/', ':']).next()?;
    let name = tail.strip_suffix(".git").unwrap_or(tail);
    parse_name(name)?;
    Some((raw.to_owned(), name.to_owned()))
}

/// Plan a new project from the palette's input.
///
/// Returns `Ok(None)` when the input is not something we would create, so the
/// palette simply offers no row rather than showing an error while typing.
pub(super) fn plan(roots: &[String], text: &str) -> Result<Option<NewProject>, Error> {
    let Some(root) = roots.first() else {
        return Ok(None);
    };
    let root = projects::expand_root(root)?;
    let (label, source) = match parse_repo(text) {
        Some((url, name)) => (name, Source::Clone { url }),
        None => match parse_name(text) {
            Some(name) => (name, Source::Folder),
            None => return Ok(None),
        },
    };
    Ok(Some(NewProject {
        path: root.join(&label),
        label,
        source,
    }))
}

/// Make the directory, or clone into it, returning the project to open.
/// Blocking: callers run it off the UI thread.
pub(super) fn materialise(
    project: &NewProject,
    cancelled: &impl Fn() -> bool,
) -> Result<projects::Project, Error> {
    let Some(root) = project.path.parent() else {
        return Err(Error::PaletteProjectRootMissing);
    };
    if !root.is_dir() {
        return Err(Error::PaletteProjectRootMissing);
    }
    match &project.source {
        // Claim the destination atomically: whichever attempt creates it owns
        // it, and a second fails here instead of sharing it.
        Source::Folder => fs::create_dir(&project.path).map_err(claim_error)?,
        Source::Clone { url } => clone(url, root, &project.path, &project.label, cancelled)?,
    }
    let path = fs::canonicalize(&project.path)
        .map_err(|error| Error::from(error).at_path(&project.path))?;
    Ok(projects::Project {
        path,
        label: project.label.clone(),
    })
}

/// Clone into a hidden staging folder beside the destination, then publish it.
///
/// An unfinished clone is never visible as a project: discovery skips dot
/// folders, so nobody can open it and save files that a failed clone would
/// then remove. The staging folder is created atomically under a fresh name,
/// so cleanup only ever removes the folder this attempt made.
fn clone(
    url: &str,
    root: &Path,
    destination: &Path,
    label: &str,
    cancelled: &impl Fn() -> bool,
) -> Result<(), Error> {
    // Not a guarantee (`publish` is), only a way not to download a repository
    // that has nowhere to go.
    if destination.symlink_metadata().is_ok() {
        return Err(Error::PaletteProjectExists);
    }
    let staging = staging_dir(root, label)?;
    let deadline = Instant::now() + CLONE_TIMEOUT;
    // `git clone` accepts a destination that exists and is empty.
    let mut command = clone_command(url, staging.path());
    match run(&mut command, deadline, cancelled)? {
        (true, _) => {}
        (false, output) => return Err(Error::PaletteProjectClone(output.trim().to_owned())),
    }
    publish(staging.path(), destination)?;
    // Moved into place: there is nothing left for the guard to remove.
    let _ = staging.keep();
    Ok(())
}

/// A uniquely named hidden folder in `root`, removed when dropped.
pub(super) fn staging_dir(root: &Path, label: &str) -> Result<tempfile::TempDir, Error> {
    let prefix = format!(".{label}.clone-");
    let mut builder = tempfile::Builder::new();
    builder.prefix(&prefix);
    // The folder becomes the project, so it gets ordinary umask-derived
    // permissions rather than tempfile's private 0700.
    #[cfg(unix)]
    builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o777));
    builder
        .tempdir_in(root)
        .map_err(|source| Error::PaletteProjectCreate { source })
}

/// Move a finished clone to `destination` without replacing anything there.
///
/// `rename` replaces an empty directory on Unix, so the destination is first
/// claimed with an atomic `create_dir`: an existing folder, empty or not, is
/// refused. The rename then replaces only the empty folder just claimed, and
/// fails rather than replacing it if anything was written there meanwhile.
#[cfg(unix)]
pub(super) fn publish(staging: &Path, destination: &Path) -> Result<(), Error> {
    fs::create_dir(destination).map_err(claim_error)?;
    fs::rename(staging, destination).map_err(|source| {
        // Non-recursive, so this removes the claim only while it is still empty.
        let _ = fs::remove_dir(destination);
        Error::PaletteProjectCreate { source }
    })
}

/// Windows never renames a directory over an existing one.
#[cfg(windows)]
pub(super) fn publish(staging: &Path, destination: &Path) -> Result<(), Error> {
    if destination.symlink_metadata().is_ok() {
        return Err(Error::PaletteProjectExists);
    }
    fs::rename(staging, destination).map_err(claim_error)
}

/// An existing name is the ordinary case; anything else is a real failure.
fn claim_error(source: std::io::Error) -> Error {
    if source.kind() == std::io::ErrorKind::AlreadyExists {
        Error::PaletteProjectExists
    } else {
        Error::PaletteProjectCreate { source }
    }
}

/// The `git clone` invocation, kept apart so its arguments can be asserted
/// without a network or a repository.
pub(super) fn clone_command(url: &str, destination: &Path) -> Command {
    let mut command = Command::new("git");
    // The GUI has nowhere to show a credential prompt, so never block on one.
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("clone")
        // `--` so a URL that begins with a dash is treated as data.
        .arg("--")
        .arg(url)
        .arg(destination);
    command
}

/// The palette row offering to make the typed name — or clone the typed URL —
/// under the first configured project root.
fn new_project_entry(roots: &[String], query: &str) -> Option<Entry> {
    let plan = plan(roots, query).ok().flatten()?;
    let (label, detail) = match &plan.source {
        Source::Folder => (
            format!("Create folder \"{}\"", plan.label),
            format!("New project in {}", plan.path.display()),
        ),
        Source::Clone { url } => (
            format!("Clone \"{}\"", plan.label),
            format!("git clone {url} into {}", plan.path.display()),
        ),
    };
    Some(Entry::with_keywords(
        label,
        detail,
        "Project",
        Action::NewProject(plan),
        None,
        // The raw query, so the row is offered for exactly what was typed.
        query,
    ))
}

impl Palette {
    /// The base entries plus the row the current query would create, if any.
    ///
    /// `pending` is rebuilt rather than appended to, so the row can change with
    /// the query without accumulating.
    pub(super) fn rebuild_pending(&mut self, roots: &[String]) {
        let mut entries: Vec<Entry> = self.base.iter().cloned().collect();
        if !self.query.trim().is_empty()
            && let Some(entry) = new_project_entry(roots, &self.query)
        {
            entries.push(entry);
        }
        self.pending = entries.into();
    }
}

#[cfg(test)]
mod tests;
