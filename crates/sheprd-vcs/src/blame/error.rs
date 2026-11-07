//! Why a file's blame could not be read.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not write the file's text for Git to read.")]
    Contents(#[source] std::io::Error),
    #[error("This file is not in a folder Git can read.")]
    Path,
    #[error("Git does not track this file, so it has no history to show.")]
    Untracked,
    // keep-sorted end
}
