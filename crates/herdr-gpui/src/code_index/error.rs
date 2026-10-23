//! Why a checkout could not be listed and scanned for files and symbols.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Reading the checkout was cancelled")]
    Cancelled,
    #[error("Only a local Git checkout can be searched for files and symbols")]
    Root,
    // keep-sorted end
}
