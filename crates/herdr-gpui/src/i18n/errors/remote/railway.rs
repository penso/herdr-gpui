//! Railway VMs.

use super::path;
use crate::i18n::{errors::Localize, fl};
use herdr_remote::railway::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::KeygenMissing(_) => fl!("error-remote-railway-keygen-missing"),
            // As `Display` writes it: `Some(1)`, or `None` for a signal.
            Self::Keygen(code) => {
                fl!("error-remote-railway-keygen", status = format!("{code:?}"))
            }
            Self::KeygenTimeout => fl!("error-remote-railway-keygen-timeout"),
            Self::UnusableKey(key) => fl!("error-remote-railway-unusable-key", path = path(key)),
            Self::KeyName => fl!("error-remote-railway-key-name"),
            Self::KeyExists(key) => fl!("error-remote-railway-key-exists", path = path(key)),
            Self::KeyPath => fl!("error-remote-railway-key-path"),
            Self::KeyFile { path: key, .. } => {
                fl!("error-remote-railway-key-file", path = path(key))
            }
            Self::Store { path: file, .. } => fl!("error-remote-railway-store", path = path(file)),
            Self::NoIdentity => fl!("error-remote-railway-no-identity"),
            Self::KeyMissing(key) => fl!("error-remote-railway-key-missing", path = path(key)),
            Self::KeyChanged(key) => fl!("error-remote-railway-key-changed", path = path(key)),
            Self::PublicKeyMismatch(key) => {
                fl!("error-remote-railway-public-key-mismatch", path = path(key))
            }
            Self::Terminal(_) => fl!("error-remote-railway-terminal"),
            Self::TerminalUnsupported => fl!("error-remote-railway-terminal-unsupported"),
            Self::CreateTimeout => fl!("error-remote-railway-create-timeout"),
            Self::CreateRefused(detail) => fl!(
                "error-remote-railway-create-refused",
                detail = detail.as_str()
            ),
            Self::Unreachable(detail) => {
                fl!("error-remote-railway-unreachable", detail = detail.as_str())
            }
            Self::Bridge(error) => error.localize(),
            Self::Cloud(error) => error.localize(),
        }
    }
}
