//! Sandboxes on a self-hosted OpenSandbox server.

use crate::i18n::{errors::Localize, fl};
use herdr_remote::opensandbox::{Error, Status};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Url(field) => fl!("error-remote-opensandbox-url", field = *field),
            Self::Field(value) => super::value::invalid(*value),
            Self::Encoding(value) => super::value::not_utf8(*value),
            Self::MissingKey => fl!("error-remote-opensandbox-missing-key"),
            Self::Authentication => fl!("error-remote-opensandbox-authentication"),
            Self::Forbidden => fl!("error-remote-opensandbox-forbidden"),
            Self::Gone(id) => fl!("error-remote-opensandbox-gone", id = id.as_str()),
            Self::Stopped(detail) => {
                fl!("error-remote-opensandbox-stopped", detail = detail.as_str())
            }
            Self::StartTimeout => fl!("error-remote-opensandbox-start-timeout"),
            Self::Status(status) => status.localize(),
            Self::Network(_) => fl!("error-remote-opensandbox-network"),
            Self::Read(_) => fl!("error-remote-opensandbox-read"),
            Self::Size => fl!("error-remote-opensandbox-size"),
            Self::Json(_) => fl!("error-remote-opensandbox-json"),
            Self::Header(_) => fl!("error-remote-opensandbox-header"),
            Self::Socket(error) => error.localize(),
            Self::Shell(detail) => {
                fl!("error-remote-opensandbox-shell", detail = detail.as_str())
            }
            Self::Output(_) => fl!("error-remote-opensandbox-output"),
            Self::Executable(_) => fl!("error-remote-opensandbox-executable"),
            Self::Cancelled => fl!("error-remote-opensandbox-cancelled"),
            Self::Account => fl!("error-remote-opensandbox-account"),
            Self::Bridge(error) => error.localize(),
            Self::Cloud(error) => error.localize(),
            Self::Storage(error) => error.localize(),
        }
    }
}

impl Localize for Status {
    fn localize(&self) -> String {
        match &self.message {
            Some(detail) => fl!(
                "error-remote-opensandbox-status-message",
                code = self.code,
                detail = detail.as_str()
            ),
            None => fl!("error-remote-opensandbox-status", code = self.code),
        }
    }
}
