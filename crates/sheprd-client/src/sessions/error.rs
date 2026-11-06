//! Why listing or deleting a machine's named sessions failed.

use std::{io, process::ExitStatus};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("The default session cannot be deleted")]
    DeleteDefault,
    #[error(
        "Herdr refused session deletion ({0}); the session may be running, inaccessible, or unsupported by the installed CLI"
    )]
    DeleteFailed(ExitStatus),
    #[error("Session deletion timed out; refresh the list before trying again")]
    DeleteTimeout,
    #[error("too many sessions to list")]
    Limit,
    // keep-sorted end
}

impl Error {
    /// The transport retry/cancellation category `crate::Error::kind` reports.
    pub(crate) fn kind(&self) -> io::ErrorKind {
        match self {
            Self::DeleteDefault => io::ErrorKind::InvalidInput,
            Self::DeleteTimeout => io::ErrorKind::TimedOut,
            Self::DeleteFailed(_) | Self::Limit => io::ErrorKind::InvalidData,
        }
    }
}
