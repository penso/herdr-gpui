use std::{io, path::PathBuf, process::ExitStatus};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("remote.certificate {which} must be an absolute UTF-8 path")]
    CertificatePath { which: &'static str },
    #[error(transparent)]
    Control(#[from] crate::control::Error),
    #[error("remote.device_idle_days must be between 0 (never) and 365")]
    IdleDays,
    #[error("Could not {operation} {}", path.display())]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Herdr UI has no state directory for remote control")]
    NoStateDir,
    #[error("{} cannot go in a service definition", path.display())]
    PathText { path: PathBuf },
    #[error("remote.port must be between 1 and 65535")]
    Port,
    #[error("herdr-remote-control was not found beside Herdr UI or on PATH")]
    ProgramMissing,
    #[error("The pairing link does not fit in a QR code")]
    Qr(#[source] qrcode::types::QrError),
    #[error("{program} failed ({status}): {stderr}")]
    Service {
        program: &'static str,
        status: ExitStatus,
        /// The command's own words, cut short, for the log and Settings.
        stderr: String,
    },
    #[error("Could not run {program}")]
    Spawn {
        program: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("The remote control daemon gave an answer of the wrong kind")]
    Unexpected,
    #[error("Windows did not say who the current user is")]
    UnknownUser,
    #[error("Remote control cannot run as a service on this platform")]
    Unsupported,
    // keep-sorted end
}
