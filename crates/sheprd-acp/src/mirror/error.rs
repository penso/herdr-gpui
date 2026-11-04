use std::io;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Why a watched transcript could not be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the transcript could not be read")]
    Io(#[from] io::Error),
    #[error("the transcript is not a regular file")]
    NotRegularFile,
}
