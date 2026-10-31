//! Boat sandboxes.

use super::path;
use crate::i18n::{errors::Localize, fl};
use herdr_remote::boat::{Error, Status};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Url(field) => fl!("error-remote-boat-url", field = *field),
            Self::Field(value) => super::value::invalid(*value),
            Self::Encoding(value) => super::value::not_utf8(*value),
            Self::MissingKey => fl!("error-remote-boat-missing-key"),
            Self::Authentication => fl!("error-remote-boat-authentication"),
            Self::Forbidden => fl!("error-remote-boat-forbidden"),
            Self::Status(status) => status.localize(),
            Self::Network(_) => fl!("error-remote-boat-network"),
            Self::Read(_) => fl!("error-remote-boat-read"),
            Self::Size => fl!("error-remote-boat-size"),
            Self::Json(_) => fl!("error-remote-boat-json"),
            Self::Header(_) => fl!("error-remote-boat-header"),
            Self::Sandbox(detail) => fl!("error-remote-boat-sandbox", detail = detail.as_str()),
            Self::Deleted => fl!("error-remote-boat-deleted"),
            Self::StartTimeout => fl!("error-remote-boat-start-timeout"),
            Self::Cancelled => fl!("error-remote-boat-cancelled"),
            Self::PendingIo { path: file, source } => fl!(
                "error-remote-boat-pending-io",
                path = path(file),
                error = source.to_string()
            ),
            Self::PendingBusy => fl!("error-remote-boat-pending-busy"),
            Self::PendingExpired => fl!("error-remote-boat-pending-expired"),
            Self::PendingCreation(error) => {
                fl!(
                    "error-remote-boat-pending-creation",
                    error = error.localize()
                )
            }
            // The system's own words about its clock.
            Self::Clock(source) => fl!("error-remote-boat-clock", error = source.to_string()),
            Self::Gateway => fl!("error-remote-boat-gateway"),
            Self::SshMissing(_) => fl!("error-remote-boat-ssh-missing"),
            Self::SshKey(_) => fl!("error-remote-boat-ssh-key"),
            Self::SshKeygen => fl!("error-remote-boat-ssh-keygen"),
            Self::GatewayFile(_) => fl!("error-remote-boat-gateway-file"),
            Self::SaveDevice { id, source } => fl!(
                "error-remote-boat-save-device",
                id = id.as_str(),
                error = source.localize()
            ),
            Self::Setup { id, source } => fl!(
                "error-remote-boat-setup",
                id = id.as_str(),
                error = source.localize()
            ),
            Self::Account => fl!("error-remote-boat-account"),
            Self::Bridge(error) => error.localize(),
            Self::Cloud(error) => error.localize(),
            Self::KeyFile(error) => fl!("error-remote-boat-key-file", error = error.localize()),
            Self::Storage(error) => error.localize(),
        }
    }
}

impl Localize for Status {
    fn localize(&self) -> String {
        match &self.message {
            Some(detail) => fl!(
                "error-remote-boat-status-message",
                code = self.code,
                detail = detail.as_str()
            ),
            None => fl!("error-remote-boat-status", code = self.code),
        }
    }
}
