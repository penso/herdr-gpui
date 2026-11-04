use std::{io, path::PathBuf};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Failures reaching or serving the control socket, and an app's refusal of
/// a request. The display text is what `herdr-gpui <command>` prints.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("Control request or response exceeds {limit} bytes")]
    MessageSize { limit: usize },
    #[error("Sheprd closed the control connection without answering")]
    NoResponse,
    #[error("{} is held by something other than this user's Sheprd, so it is not used", path.display())]
    PipeForeign { path: PathBuf },
    #[error("{message}")]
    Rejected {
        code: crate::protocol::ErrorCode,
        message: String,
    },
    #[error("Another Sheprd already serves {}", path.display())]
    SocketInUse { path: PathBuf },
    #[error("Sheprd is not running, or its control socket {} is unreachable: {source}", path.display())]
    Unavailable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Could not identify the Windows user: {0}")]
    User(#[source] io::Error),
    // keep-sorted end
}
