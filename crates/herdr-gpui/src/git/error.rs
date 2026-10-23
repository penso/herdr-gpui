//! Why a Git status read, commit, push, or pull request creation failed.

use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("A Git operation is already running.")]
    Busy,
    #[error("Enter a commit message of at most 4096 characters.")]
    CommitMessage,
    #[error("Could not run git in {}: {source}", path.display())]
    CommonDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("git {operation} failed: {details}")]
    Failed {
        operation: &'static str,
        details: String,
    },
    #[error("No local checkout is focused.")]
    NoCheckout,
    #[error("Nothing to commit: the checkout has no changes.")]
    NothingToCommit,
    #[error("Could not {operation}.")]
    Process {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("This branch is the repository default branch; no pull request can be opened from it.")]
    PullRequestBase,
    #[error("The branch has no commit to describe. Commit before opening a pull request.")]
    PullRequestTitle,
    #[error("Git worker stopped. Retry the operation.")]
    Worker,
    // keep-sorted end
}
