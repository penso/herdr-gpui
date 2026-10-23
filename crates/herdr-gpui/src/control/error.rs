//! Why talking to the app over its control socket failed.

use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Control request or response exceeds {limit} bytes")]
    MessageSize { limit: usize },
    #[error("Herdr UI closed the control connection without answering")]
    NoResponse,
    #[error("{} is held by something other than this user's Herdr UI, so it is not used", path.display())]
    PipeForeign { path: PathBuf },
    #[error("{message}")]
    Rejected {
        code: crate::control::ErrorCode,
        message: String,
    },
    #[error("Another Herdr UI already serves {}", path.display())]
    SocketInUse { path: PathBuf },
    #[error("Herdr UI is not running, or its control socket {} is unreachable: {source}", path.display())]
    Unavailable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Could not identify the Windows user: {0}")]
    User(#[source] io::Error),
    // keep-sorted end
}
