//! The VS Code server: `error-code-server.ftl`. Like the `Display`, no
//! message names the connection token.

use super::Localize;
use crate::{
    code_server::{Error, NoAnswer},
    i18n::fl,
};

/// The selector key for why no answer came.
fn reason(reason: NoAnswer) -> &'static str {
    match reason {
        NoAnswer::Refused => "refused",
        NoAnswer::Timeout => "timeout",
        NoAnswer::UnknownHost => "unknown-host",
        NoAnswer::Other => "other",
    }
}

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Unreachable {
                address, reason: r, ..
            } => fl!(
                "error-code-server-unreachable",
                reason = reason(*r),
                address = address.as_str()
            ),
            // Ports and HTTP statuses are identifiers, not quantities.
            Self::NotServer { status } => {
                fl!("error-code-server-not-server", status = status.to_string())
            }
            Self::TokenRefused => fl!("error-code-server-token-refused"),
            Self::Status(status) => fl!("error-code-server-status", status = status.to_string()),
            Self::NoDataDir => fl!("error-code-server-no-data-dir"),
            Self::TokenFile { path, .. } => {
                fl!(
                    "error-code-server-token-file",
                    path = path.display().to_string()
                )
            }
            Self::TokenRandom(_) => fl!("error-code-server-token-random"),
            Self::NoFreePort(_) => fl!("error-code-server-no-free-port"),
            Self::PortTaken { port } => {
                fl!("error-code-server-port-taken", port = port.to_string())
            }
            Self::PortCheck { port, .. } => {
                fl!("error-code-server-port-check", port = port.to_string())
            }
            Self::Spawn { program, .. } => fl!(
                "error-code-server-spawn",
                program = program.display().to_string()
            ),
            // The system's words for how the process ended.
            Self::Exited(status) => fl!("error-code-server-exited", status = status.to_string()),
            Self::Watch(_) => fl!("error-code-server-watch"),
            Self::Silent(wait) => fl!("error-code-server-silent", seconds = wait.as_secs()),
            Self::Worker(_) => fl!("error-code-server-worker"),
        }
    }
}
