//! Why a clipboard image upload did not reach the daemon.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("a clipboard image upload is already reserved or in progress")]
    Busy,
    #[error("clipboard image upload cancelled")]
    Cancelled,
    #[error("semantic input fallback requires a pane or popup target")]
    InputTarget,
    #[error("clipboard image preparation timed out; reservation skipped")]
    PreparationTimeout,
    #[error("clipboard image write timed out; reconnect required")]
    WriteTimeout,
    // keep-sorted end
}

impl Error {
    /// The transport retry/cancellation category `crate::Error::kind` reports.
    pub(crate) fn kind(&self) -> io::ErrorKind {
        match self {
            Self::Busy => io::ErrorKind::WouldBlock,
            Self::Cancelled => io::ErrorKind::Interrupted,
            Self::PreparationTimeout | Self::WriteTimeout => io::ErrorKind::TimedOut,
            Self::InputTarget => io::ErrorKind::InvalidData,
        }
    }
}
