//! Why reaching or managing a WSL distribution failed.

use std::{io, path::PathBuf, process::ExitStatus};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("unsupported WSL device list version, size, or entries")]
    Catalog,
    #[error("invalid WSL device list schema")]
    CatalogSchema(#[source] serde_json::Error),
    #[error("WSL bridge closed; check that the distribution starts and Herdr is installed in it")]
    Closed,
    #[error("wsl.exe failed ({0})")]
    Command(ExitStatus),
    #[error("this WSL distribution is already a device")]
    HostExists,
    #[error("this WSL distribution is not a saved device")]
    HostMissing,
    #[error("invalid WSL distribution name")]
    InvalidDistro,
    #[error("{} is not an absolute Windows path a WSL distribution can map", .0.display())]
    PathInvalid(PathBuf),
    #[error("a WSL distribution takes at most 256 dropped paths at once")]
    PathLimit,
    #[error("invalid answer from wslpath")]
    PathResponse,
    #[error("{} is not reachable from this WSL distribution; only its own files and mounted drives are", .0.display())]
    PathUnmapped(PathBuf),
    #[error("WSL did not answer in time; the distribution may be stopped or waiting for setup")]
    Timeout,
    #[error("WSL distributions are only available on Windows")]
    Unsupported,
    // keep-sorted end
}

impl Error {
    /// The transport retry/cancellation category `crate::Error::kind` reports.
    pub(crate) fn kind(&self) -> io::ErrorKind {
        match self {
            Self::InvalidDistro | Self::PathLimit | Self::PathInvalid(_) => {
                io::ErrorKind::InvalidInput
            }
            Self::PathUnmapped(_) => io::ErrorKind::NotFound,
            Self::Unsupported => io::ErrorKind::Unsupported,
            Self::Closed => io::ErrorKind::UnexpectedEof,
            Self::Timeout => io::ErrorKind::TimedOut,
            Self::Catalog
            | Self::CatalogSchema(_)
            | Self::Command(_)
            | Self::HostExists
            | Self::HostMissing
            | Self::PathResponse => io::ErrorKind::InvalidData,
        }
    }
}
