//! Why looking up a branch's pull request failed.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Daemon did not provide an absolute checkout and repository key.")]
    AbsolutePath,
    #[error("Multiple PRs match this branch; no PR selected.")]
    Ambiguous,
    #[error("Multiple local worktrees match the daemon branch.")]
    AmbiguousWorktree,
    #[error("No supported branch available.")]
    Branch,
    #[error("Checkout branch changed. Waiting for daemon metadata.")]
    BranchChanged,
    #[error("PR lookup cancelled.")]
    Cancelled,
    #[error("Local checkout unavailable or not a trusted Git repository.")]
    Checkout,
    #[error("Invalid process text.")]
    Encoding(#[source] std::str::Utf8Error),
    #[error("PR identity does not match the repository and branch.")]
    Identity,
    #[error("No repository metadata.")]
    Metadata,
    #[error("No local worktree matches the daemon branch.")]
    MissingWorktree,
    #[error("PR lookup supports GitHub.com origins only.")]
    Origin,
    #[error("Could not {operation} for Git PR lookup.")]
    Process {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("GitHub repository unavailable. Check repository access and token permissions.")]
    Repository,
    #[error("Local repository does not match daemon metadata.")]
    RepositoryMismatch,
    #[error("PR response exceeded the size limit.")]
    Size,
    #[error("PR lookup timed out (15 seconds).")]
    Timeout,
    #[error(
        "PR lookup requires your owned local session socket or a saved SSH device. Other socket locations are unsupported."
    )]
    UntrustedEndpoint,
    #[error("Workspace directory is not in a Git repository.")]
    WorkspaceRepository,
    #[error("Local repository unavailable for worktree lookup.")]
    WorktreeLookup,
    // keep-sorted end
}
