//! Committing a checkout: everything in it, or only what is staged.
use super::{Error, Outcome, checkout_status, clean, git};
use std::{
    hash::{DefaultHasher, Hash as _, Hasher as _},
    time::Instant,
};

/// How many files the next commit would take, and a digest of what it would
/// take from each. `--raw` names each file's staged blob, so the digest moves
/// whenever the index does. Records are NUL-separated pairs of those details
/// and a name.
pub(super) fn staged(
    checkout: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<(u64, u64)> {
    let raw = git(
        checkout,
        &[
            "diff",
            "--cached",
            "--raw",
            "--no-abbrev",
            "--no-renames",
            "-z",
        ],
        "read staged changes",
        deadline,
        cancelled,
    )?;
    let fields = raw.split('\0').filter(|field| !field.is_empty()).count();
    let mut digest = DefaultHasher::new();
    raw.hash(&mut digest);
    Ok((fields as u64 / 2, digest.finish()))
}

pub(super) fn all(
    checkout: &str,
    message: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Outcome> {
    if !checkout_status(checkout, deadline, cancelled)?.dirty() {
        return Err(Error::NothingToCommit.into());
    }
    git(
        checkout,
        &["add", "-A"],
        "stage changes",
        deadline,
        cancelled,
    )?;
    index(checkout, message, deadline, cancelled)
}

/// Commits the index as it stands, so a hunk staged from a review goes in
/// without the changes around it.
pub(super) fn only_staged(
    checkout: &str,
    message: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Outcome> {
    if staged(checkout, deadline, cancelled)?.0 == 0 {
        return Err(Error::NothingStaged.into());
    }
    index(checkout, message, deadline, cancelled)
}

fn index(
    checkout: &str,
    message: &str,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> crate::Result<Outcome> {
    git(
        checkout,
        &["commit", "-m", message],
        "commit",
        deadline,
        cancelled,
    )?;
    let head = git(
        checkout,
        &["log", "-1", "--pretty=format:%h %s"],
        "read the new commit",
        deadline,
        cancelled,
    )?;
    Ok(Outcome {
        message: format!("Committed {}", clean(&head)),
        url: None,
    })
}
