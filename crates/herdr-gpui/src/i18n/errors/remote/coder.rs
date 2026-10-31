//! Coder workspaces.

use super::path;
use crate::i18n::{errors::Localize, fl};
use herdr_remote::coder::{Error, Status};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Url(field) => fl!("error-remote-coder-url", field = *field),
            Self::Field(value) => super::value::invalid(*value),
            Self::Encoding(value) => super::value::not_utf8(*value),
            Self::Missing(key) => fl!("error-remote-coder-missing", key = *key),
            Self::Authentication => fl!("error-remote-coder-authentication"),
            Self::Client => fl!("error-remote-coder-client"),
            Self::Forbidden => fl!("error-remote-coder-forbidden"),
            Self::Status(status) => status.localize(),
            Self::Network(_) => fl!("error-remote-coder-network"),
            Self::Read(_) => fl!("error-remote-coder-read"),
            Self::Size => fl!("error-remote-coder-size"),
            Self::Json(_) => fl!("error-remote-coder-json"),
            Self::Header(_) => fl!("error-remote-coder-header"),
            Self::Listen { address, .. } => {
                fl!("error-remote-coder-listen", address = address.as_str())
            }
            Self::Redirect(_) => fl!("error-remote-coder-redirect"),
            Self::Cancelled => fl!("error-remote-coder-cancelled"),
            Self::Timeout => fl!("error-remote-coder-timeout"),
            Self::Authorization(detail) => {
                fl!("error-remote-coder-authorization", detail = detail.as_str())
            }
            Self::Token => fl!("error-remote-coder-token"),
            Self::Random(_) => fl!("error-remote-coder-random"),
            Self::Workspace(detail) => {
                fl!("error-remote-coder-workspace", detail = detail.as_str())
            }
            Self::Deleted => fl!("error-remote-coder-deleted"),
            Self::BuildTimeout => fl!("error-remote-coder-build-timeout"),
            Self::DeleteTimeout => fl!("error-remote-coder-delete-timeout"),
            Self::Cli => fl!("error-remote-coder-cli"),
            Self::Deployment => fl!("error-remote-coder-deployment"),
            Self::Bridge(error) => error.localize(),
            Self::Catalog { path: file, source } => fl!(
                "error-remote-coder-catalog",
                path = path(file),
                error = source.to_string()
            ),
            Self::Cloud(error) => error.localize(),
            Self::Storage(error) => error.localize(),
            Self::Worker(worker) => fl!(
                "error-remote-coder-worker",
                worker = super::cloud::worker_selector(*worker)
            ),
            Self::AmbiguousTemplate { name, matches } => fl!(
                "error-remote-coder-ambiguous-template",
                name = name.as_str(),
                matches = matches.as_str()
            ),
            Self::Template {
                name: Some(name),
                available,
            } => fl!(
                "error-remote-coder-template",
                name = name.as_str(),
                available = available.as_str()
            ),
            Self::Template {
                name: None,
                available,
            } => fl!(
                "error-remote-coder-template-unnamed",
                available = available.as_str()
            ),
            Self::Preset {
                template,
                name,
                available,
            } => fl!(
                "error-remote-coder-preset",
                template = template.as_str(),
                name = name.as_str(),
                available = available.as_str()
            ),
        }
    }
}

impl Localize for Status {
    fn localize(&self) -> String {
        match &self.message {
            Some(detail) => fl!(
                "error-remote-coder-status-message",
                code = self.code,
                detail = detail.as_str()
            ),
            None => fl!("error-remote-coder-status", code = self.code),
        }
    }
}
