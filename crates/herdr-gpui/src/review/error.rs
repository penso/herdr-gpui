//! Why a review's changes could not be shown.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(
        "No base branch to compare with: neither the pull request's base nor origin/HEAD, main, or master exists locally. Pick one with the base button above."
    )]
    NoBase,
    #[error("These changes are too large to review here, even leaving out the largest files.")]
    TooLarge,
    // keep-sorted end
}
