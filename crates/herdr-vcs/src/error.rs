//! `herdr-vcs` failures: each feature's own, and those shared between them.

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Checkpoint(#[from] crate::checkpoint::Error),
    /// Only CPU and memory are sampled on an Azure machine so far.
    #[cfg(feature = "azure")]
    #[error("This is not available for cloud machines yet.")]
    CloudHostUnsupported,
    #[error(transparent)]
    CodeIndex(#[from] crate::code_index::Error),
    #[error(transparent)]
    Git(#[from] crate::git::Error),
    #[error(transparent)]
    GitHub(#[from] herdr_github::Error),
    // keep-sorted end
}
