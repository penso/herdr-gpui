//! Why a comment, merge, or review read on a pull request could not go ahead.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("A pull request action is already running.")]
    Busy,
    #[error("Enter a comment of at most 4096 characters.")]
    CommentBody,
    #[error("The branch changed since this dialog opened. Review the pull request again.")]
    MergeChanged,
    #[error("The repository does not allow this merge method.")]
    MergeMethod,
    #[error("Could not start the pull request worker.")]
    Process {
        #[source]
        source: io::Error,
    },
    #[error(
        "This pull request cannot be acted on here: it is not open, or its details are incomplete. Refresh and try again."
    )]
    Target,
    #[error("Pull request worker stopped. Check the pull request on GitHub before retrying.")]
    Worker,
    // keep-sorted end
}
