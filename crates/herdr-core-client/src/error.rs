use crate::{Method, frame, hello};
use std::{io, path::PathBuf, process::ExitStatus};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Backend(#[from] herdr_backend::Error),
    #[error("too many herdr-core requests are waiting for an answer")]
    Busy,
    #[error(transparent)]
    Compat(#[from] hello::Mismatch),
    #[error("cannot reach herdr-core at {}", path.display())]
    Connect {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Frame(#[from] frame::Error),
    #[error("herdr-core sent an invalid {0} answer")]
    Invalid(Method, #[source] serde_json::Error),
    #[error("herdr-core I/O failed")]
    Io(#[from] io::Error),
    #[error("invalid herdr-core message")]
    Json(#[from] serde_json::Error),
    #[error("the connection to herdr-core was lost")]
    Lost,
    #[error("no home folder to find herdr-core in")]
    NoHome,
    #[error("herdr-core broke the protocol: {0}")]
    Protocol(#[from] Violation),
    /// `kind` and `message` are the core's own words, kept for diagnostics.
    #[error("herdr-core refused {method}: {message}")]
    Refused {
        method: Method,
        kind: Option<String>,
        message: String,
    },
    #[error("HC_SESSION {0:?} cannot be a folder name")]
    Session(String),
    #[error("cannot start herdr-core at {}", binary.display())]
    Start {
        binary: PathBuf,
        #[source]
        source: io::Error,
    },
    /// `stderr` is the start command's bounded output, for diagnostics.
    #[error("herdr-core did not start ({status})")]
    StartFailed { status: ExitStatus, stderr: String },
    #[error("herdr-core did not start within {0:?}")]
    StartTimeout(std::time::Duration),
    #[error("herdr-core did not answer {0} in time")]
    Timeout(Method),
    // keep-sorted end
}

/// How the core broke the protocol; the connection ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Violation {
    // keep-sorted start
    #[error("a hello reply longer than {0} bytes")]
    LongHello(usize),
    #[error("an event line longer than {0} bytes")]
    LongLine(usize),
    #[error("a stream sent past its credit")]
    PastCredit,
    #[error("the hello reply was not framed")]
    Unframed,
    // keep-sorted end
}
