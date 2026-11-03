//! Test fixture asking Git itself whether it takes a branch name, so the
//! branch field's formatting is checked against `git check-ref-format`
//! rather than only against this client's copy of its rules.

#![allow(clippy::expect_used)]

/// Whether Git takes `branch` as a branch name, as
/// `git check-ref-format --branch` judges it. Panics when `git` cannot run,
/// so a missing binary is not mistaken for a name Git refuses.
pub(crate) fn git_accepts(branch: &str) -> bool {
    std::process::Command::new("git")
        .args(["check-ref-format", "--branch", branch])
        .output()
        .expect("`git` runs, as the branch formatting tests ask it to judge names")
        .status
        .success()
}
