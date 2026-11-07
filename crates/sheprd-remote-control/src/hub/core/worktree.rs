//! Worktrees for new work on a core. The core keeps no workspaces or
//! worktrees, so the daemon makes the checkout itself where Herdr would,
//! and the chat simply starts in it.

mod error;

pub use error::Error;

use sheprd_client::{local_path::home_dir, new_command};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Stdio,
};

/// The most of Git's complaint that is kept for the phone and the log.
const MAX_COMPLAINT_CHARS: usize = 300;
/// The folder under home that Herdr's default keeps checkouts in.
const ROOT: &str = ".herdr/worktrees";
/// The folder of a branch with no ASCII letters or digits.
const FALLBACK_SLUG: &str = "worktree";

/// Where new checkouts go, when this user has a home.
pub(in crate::hub) fn root() -> Option<PathBuf> {
    home_dir().map(|home| home.join(ROOT))
}

/// Adds a new `branch` from what `repository` has checked out, under
/// `root`, and answers the checkout's folder.
pub(in crate::hub) fn add(repository: &Path, branch: &str, root: &Path) -> Result<PathBuf, Error> {
    let common = git(
        repository,
        [
            OsStr::new("rev-parse"),
            OsStr::new("--path-format=absolute"),
            OsStr::new("--git-common-dir"),
        ],
    )?;
    let name = repository_name(Path::new(common.trim())).ok_or(Error::NoRepositoryName)?;
    let checkout = root.join(name).join(path_slug(branch));
    git(
        repository,
        [
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("-b"),
            OsStr::new(branch),
            checkout.as_os_str(),
            OsStr::new("HEAD"),
        ],
    )?;
    Ok(checkout)
}

/// The repository's folder name, from its common Git directory: the folder
/// holding `.git`, or a bare repository's own name without `.git`.
fn repository_name(common: &Path) -> Option<&OsStr> {
    let name = common.file_name()?;
    if name == ".git" {
        return common.parent()?.file_name();
    }
    Some(common.file_stem().unwrap_or(name))
}

/// Herdr's folder name for `branch`: lowercase ASCII letters and digits,
/// with every other run of characters one dash and none at either end.
fn path_slug(branch: &str) -> String {
    let lower = branch.to_ascii_lowercase();
    let words = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty());
    let slug = words.collect::<Vec<_>>().join("-");
    if slug.is_empty() {
        FALLBACK_SLUG.to_owned()
    } else {
        slug
    }
}

fn git<'a>(repository: &Path, args: impl IntoIterator<Item = &'a OsStr>) -> Result<String, Error> {
    let output = new_command("git")
        .current_dir(repository)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(Error::Start)?;
    if !output.status.success() {
        let complaint = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Refused(
            complaint.trim().chars().take(MAX_COMPLAINT_CHARS).collect(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(in crate::hub) mod tests;
