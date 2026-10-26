use std::{io, num::ParseIntError};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Why a running agent's transcript could not be identified or kept.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot uniquely identify the running agent's transcript")]
    Ambiguous,
    #[error("agent config directory is not valid UTF-8")]
    ConfigDirNotUtf8,
    #[error("Codex transcript directory exceeds the observation limit")]
    DayTooLarge,
    #[error("process environment exceeds the observation limit")]
    EnvironmentTooLarge,
    #[error("the original agent exited or was replaced")]
    Exited,
    #[error("invalid process start time")]
    InvalidStartTime(#[source] ParseIntError),
    #[error("invalid process stat")]
    InvalidStat,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Mirror(#[from] crate::mirror::Error),
    #[error("missing process start time")]
    MissingStartTime,
    #[error("Codex session has no creation timestamp")]
    NoTimestamp,
    #[error("Codex transcript lookup without a descriptor requires a UUIDv7 session")]
    NotUuidV7,
    #[error("the original agent process has changed")]
    ProcessChanged,
    // Only Unix exposes the inode identity that tells a rotated log apart.
    #[cfg(unix)]
    #[error("the transcript rotated; reconnect to verify its new file")]
    Rotated,
    #[error("the session id is not a UUID")]
    Session(#[from] uuid::Error),
    #[error("the pinned transcript no longer describes the original session")]
    SessionChanged,
    #[error("Codex session creation timestamp is out of range")]
    TimestampRange,
    #[error("existing terminal observation requires Linux process metadata")]
    Unsupported,
}
