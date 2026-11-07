//! boxd.

use crate::i18n::{errors::Localize, fl};
use sheprd_remote::boxd::{Code, Error, Status};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Url(field) => fl!("error-remote-boxd-url", field = *field),
            Self::Field(value) => super::value::invalid(*value),
            Self::Encoding(value) => super::value::not_utf8(*value),
            Self::MissingKey => fl!("error-remote-boxd-missing-key"),
            Self::Authentication => fl!("error-remote-boxd-authentication"),
            Self::Forbidden => fl!("error-remote-boxd-forbidden"),
            Self::Gone(name) => fl!("error-remote-boxd-gone", name = name.as_str()),
            Self::Failed(name) => fl!("error-remote-boxd-failed", name = name.as_str()),
            Self::NotReady(name) => fl!("error-remote-boxd-not-ready", name = name.as_str()),
            Self::Status(status) => status.localize(),
            Self::Network(_) => fl!("error-remote-boxd-network"),
            Self::Read(_) => fl!("error-remote-boxd-read"),
            Self::Size => fl!("error-remote-boxd-size"),
            Self::Wire => fl!("error-remote-boxd-wire"),
            Self::Header(_) => fl!("error-remote-boxd-header"),
            Self::Socket(error) => error.localize(),
            Self::Stream(_) => fl!("error-remote-boxd-stream"),
            Self::Output(_) => fl!("error-remote-boxd-output"),
            Self::Executable(_) => fl!("error-remote-boxd-executable"),
            Self::Cancelled => fl!("error-remote-boxd-cancelled"),
            Self::Account => fl!("error-remote-boxd-account"),
            Self::Bridge(error) => error.localize(),
            Self::Cloud(error) => error.localize(),
            Self::Storage(error) => error.localize(),
        }
    }
}

impl Localize for Status {
    fn localize(&self) -> String {
        match (self.code, &self.message) {
            (Code::Grpc(code), Some(detail)) => fl!(
                "error-remote-boxd-status-grpc-message",
                code = code,
                detail = detail.as_str()
            ),
            (Code::Grpc(code), None) => fl!("error-remote-boxd-status-grpc", code = code),
            (Code::Http(code), _) => fl!("error-remote-boxd-status-http", code = code),
        }
    }
}
