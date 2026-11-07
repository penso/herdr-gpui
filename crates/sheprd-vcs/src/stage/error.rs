//! Why a hunk could not be staged.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("This hunk has changed since it was read. Look at it again before staging it.")]
    Changed,
    #[error("Nothing to stage: this hunk is already staged.")]
    NothingToStage,
    #[error("Could not write the hunk for Git to apply.")]
    Patch(#[source] std::io::Error),
    // keep-sorted end
}
