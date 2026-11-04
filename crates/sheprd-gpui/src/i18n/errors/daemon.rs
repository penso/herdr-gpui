//! Starting or talking to the herdr server: `error-daemon.ftl`.

use super::Localize;
use crate::{daemon::Error, i18n::fl};

/// The daemon's own message for an error it reported, as `daemon::Error`'s
/// `Display` picks it, or `None` when it gave none.
fn reported(error: &serde_json::Value) -> Option<&str> {
    error
        .get("message")
        .and_then(serde_json::Value::as_str)
        .or_else(|| error.as_str())
        .or_else(|| error.get("code").and_then(serde_json::Value::as_str))
        .filter(|message| !message.is_empty())
}

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Cancelled => fl!("error-daemon-cancelled"),
            // The system's words for how the server ended.
            Self::Exited(status) => fl!("error-daemon-exited", status = status.to_string()),
            // The daemon's own text, kept as it is.
            Self::Response(error) => {
                reported(error).map_or_else(|| fl!("error-daemon-response"), str::to_owned)
            }
            Self::Spawn { source } => fl!("error-daemon-spawn", error = source.to_string()),
            Self::Timeout(path) => fl!("error-daemon-timeout", path = path.display().to_string()),
            // keep-sorted end
        }
    }
}
