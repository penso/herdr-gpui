//! Daytona sandboxes.

use crate::i18n::{errors::Localize, fl};
use herdr_remote::daytona::{Error, Status};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Url(field) => fl!("error-remote-daytona-url", field = *field),
            Self::Field(value) => super::value::invalid(*value),
            Self::Encoding(value) => super::value::not_utf8(*value),
            Self::MissingKey => fl!("error-remote-daytona-missing-key"),
            Self::Authentication => fl!("error-remote-daytona-authentication"),
            Self::Forbidden => fl!("error-remote-daytona-forbidden"),
            Self::Status(status) => status.localize(),
            Self::Network(_) => fl!("error-remote-daytona-network"),
            Self::Read(_) => fl!("error-remote-daytona-read"),
            Self::Size => fl!("error-remote-daytona-size"),
            Self::Json(_) => fl!("error-remote-daytona-json"),
            Self::Header(_) => fl!("error-remote-daytona-header"),
            Self::Sandbox(detail) => {
                fl!("error-remote-daytona-sandbox", detail = detail.as_str())
            }
            Self::Deleted => fl!("error-remote-daytona-deleted"),
            Self::Snapshot(detail) => {
                fl!("error-remote-daytona-snapshot", detail = detail.as_str())
            }
            Self::StartTimeout => fl!("error-remote-daytona-start-timeout"),
            Self::Cancelled => fl!("error-remote-daytona-cancelled"),
            Self::Gateway => fl!("error-remote-daytona-gateway"),
            Self::SshMissing(_) => fl!("error-remote-daytona-ssh-missing"),
            Self::GatewayFile(_) => fl!("error-remote-daytona-gateway-file"),
            Self::Account => fl!("error-remote-daytona-account"),
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
                "error-remote-daytona-status-message",
                code = self.code,
                detail = detail.as_str()
            ),
            None => fl!("error-remote-daytona-status", code = self.code),
        }
    }
}
