//! The checkout of a directory on this machine, found with Git, for the
//! commands that run in this process: `checkpoint` and `scripts list`.
use crate::{Error, Result, checkpoint::Checkout, usage::Host};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

/// One line of a Git query in `dir`, or `None` when Git refused it.
fn git(dir: &Path, args: &[&str]) -> Result<Option<String>> {
    let output = sheprd_client::new_command("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(Error::LocalGit)?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.trim_end_matches(['\n', '\r']);
    Ok((output.status.success() && !line.is_empty()).then(|| line.to_owned()))
}

/// The checkout `dir` is in, named as the daemon names it: this machine, the
/// repository's Git common directory (absolute and canonical), and the
/// checked-out branch.
pub(super) fn checkout(dir: &Path) -> Result<Checkout> {
    let common = git(dir, &["rev-parse", "--git-common-dir"])?.ok_or(Error::NotInCheckout)?;
    // Git may print it relative to `dir`.
    let common = std::fs::canonicalize(dir.join(common)).map_err(Error::LocalGit)?;
    let key = common.to_str().ok_or(Error::NotInCheckout)?;
    let branch =
        git(dir, &["symbolic-ref", "-q", "--short", "HEAD"])?.ok_or(Error::DetachedHead)?;
    Checkout::new(Host::Local, key, Some(&branch)).ok_or(Error::NotInCheckout)
}

/// The root of the checkout `dir` is in.
pub(super) fn root(dir: &Path) -> Result<PathBuf> {
    git(dir, &["rev-parse", "--show-toplevel"])?
        .map(PathBuf::from)
        .ok_or(Error::NotInCheckout)
}
