//! Why a direct terminal attach could not start or keep going.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("too many terminal scrolls are waiting")]
    Busy,
    #[error("the terminal attach has ended")]
    Ended,
    #[error("terminal attach I/O failed")]
    Io(#[from] io::Error),
    #[error("terminal attach framing failed")]
    Protocol(#[from] herdr_protocol::Error),
    /// Herdr answered the attach with `ServerShutdown`, as when another
    /// direct client holds the terminal or it does not exist. The reason is
    /// Herdr's bounded diagnostic text.
    #[error("Herdr refused the terminal attach{}", reason_suffix(.0.as_deref()))]
    Refused(Option<String>),
    /// Herdr refused the hello itself, usually a protocol version it does
    /// not speak.
    #[error("Herdr refused the terminal hello (protocol {version})")]
    Rejected { version: u32, reason: String },
    #[error("a terminal scroll must move at least one line")]
    Scroll,
    #[error("terminal size must be nonzero")]
    Size,
    #[error("Herdr answered the terminal hello with an unexpected message")]
    Unexpected,
    // keep-sorted end
}

fn reason_suffix(reason: Option<&str>) -> String {
    reason
        .map(|reason| format!(": {reason}"))
        .unwrap_or_default()
}
