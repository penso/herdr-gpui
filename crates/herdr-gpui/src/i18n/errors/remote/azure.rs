//! Azure VMs and Container Apps sandboxes.

use crate::i18n::{errors::Localize, fl};
use herdr_remote::azure::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Field(value) => super::value::invalid(*value),
            Self::Encoding(value) => super::value::not_utf8(*value),
            Self::MissingSubscription => fl!("error-remote-azure-missing-subscription"),
            Self::MissingCredentials => fl!("error-remote-azure-missing-credentials"),
            Self::Authentication(detail) => {
                fl!(
                    "error-remote-azure-authentication",
                    detail = detail.as_str()
                )
            }
            Self::Forbidden(detail) => {
                fl!("error-remote-azure-forbidden", detail = detail.as_str())
            }
            Self::Status(code, detail) => fl!(
                "error-remote-azure-status",
                code = *code,
                detail = detail.as_str()
            ),
            // The HTTP client's own words.
            Self::Network(source) => {
                fl!("error-remote-azure-network", error = source.to_string())
            }
            Self::Read(_) => fl!("error-remote-azure-read"),
            Self::Size => fl!("error-remote-azure-size"),
            Self::Json(_) => fl!("error-remote-azure-json"),
            Self::Header(_) => fl!("error-remote-azure-header"),
            Self::Vm(detail) => fl!("error-remote-azure-vm", detail = detail.as_str()),
            Self::Deleted => fl!("error-remote-azure-deleted"),
            Self::StartTimeout => fl!("error-remote-azure-start-timeout"),
            Self::Sandbox(detail) => fl!("error-remote-azure-sandbox", detail = detail.as_str()),
            Self::SandboxDeleted => fl!("error-remote-azure-sandbox-deleted"),
            Self::SandboxTimeout => fl!("error-remote-azure-sandbox-timeout"),
            Self::DeleteTimeout => fl!("error-remote-azure-delete-timeout"),
            Self::Stream(_) => fl!("error-remote-azure-stream"),
            Self::StreamHeader(_) => fl!("error-remote-azure-stream-header"),
            Self::StreamClosed => fl!("error-remote-azure-stream-closed"),
            Self::StreamMessage => fl!("error-remote-azure-stream-message"),
            Self::Tls(_) => fl!("error-remote-azure-tls"),
            Self::Executable(_) => fl!("error-remote-azure-executable"),
            Self::Cancelled => fl!("error-remote-azure-cancelled"),
            Self::Gateway => fl!("error-remote-azure-gateway"),
            Self::SshMissing(_) => fl!("error-remote-azure-ssh-missing"),
            Self::SshKeygen => fl!("error-remote-azure-ssh-keygen"),
            Self::AzMissing => fl!("error-remote-azure-az-missing"),
            Self::AzFailed(detail) => {
                fl!("error-remote-azure-az-failed", detail = detail.as_str())
            }
            Self::Process(_) => fl!("error-remote-azure-process"),
            Self::Bridge(error) => error.localize(),
            // A boxed store failure: the keychain's or the file system's.
            Self::Storage(source) => {
                fl!("error-remote-azure-storage", error = source.to_string())
            }
            Self::Cloud(error) => error.localize(),
            Self::SaveDevice { id, source } => fl!(
                "error-remote-azure-save-device",
                id = id.as_str(),
                error = source.localize()
            ),
            Self::Account => fl!("error-remote-azure-account"),
            Self::NotRunning => fl!("error-remote-azure-not-running"),
            Self::SkuNotAvailable(detail) => fl!(
                "error-remote-azure-sku-not-available",
                detail = detail.as_str()
            ),
        }
    }
}
