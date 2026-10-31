//! Fly Sprites.

use crate::i18n::{errors::Localize, fl};
use herdr_remote::sprites::{Error, Status};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Url(field) => fl!("error-remote-sprites-url", field = *field),
            Self::Field(value) => super::value::invalid(*value),
            Self::Encoding(value) => super::value::not_utf8(*value),
            Self::MissingToken => fl!("error-remote-sprites-missing-token"),
            Self::Authentication => fl!("error-remote-sprites-authentication"),
            Self::Forbidden => fl!("error-remote-sprites-forbidden"),
            Self::Gone(name) => fl!("error-remote-sprites-gone", name = name.as_str()),
            Self::Status(status) => status.localize(),
            Self::Network(_) => fl!("error-remote-sprites-network"),
            Self::Read(_) => fl!("error-remote-sprites-read"),
            Self::Size => fl!("error-remote-sprites-size"),
            Self::Json(_) => fl!("error-remote-sprites-json"),
            Self::Header(_) => fl!("error-remote-sprites-header"),
            Self::Socket(error) => error.localize(),
            Self::Output(_) => fl!("error-remote-sprites-output"),
            Self::Executable(_) => fl!("error-remote-sprites-executable"),
            Self::Cancelled => fl!("error-remote-sprites-cancelled"),
            Self::Account => fl!("error-remote-sprites-account"),
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
                "error-remote-sprites-status-message",
                code = self.code,
                detail = detail.as_str()
            ),
            None => fl!("error-remote-sprites-status", code = self.code),
        }
    }
}
