//! `herdr_client::wsl::Error`: reaching or managing a WSL distribution,
//! from `error-client-wsl.ftl`.

use super::Localize;
use crate::i18n::fl;
use herdr_client::wsl::Error;
use std::path::Path;

fn path(path: &Path) -> String {
    path.display().to_string()
}

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Catalog => fl!("error-client-wsl-catalog"),
            Self::CatalogSchema(_) => fl!("error-client-wsl-catalog-schema"),
            Self::Closed => fl!("error-client-wsl-closed"),
            Self::Command(status) => {
                fl!("error-client-wsl-command", status = status.to_string())
            }
            Self::HostExists => fl!("error-client-wsl-host-exists"),
            Self::HostMissing => fl!("error-client-wsl-host-missing"),
            Self::InvalidDistro => fl!("error-client-wsl-invalid-distro"),
            Self::PathInvalid(file) => fl!("error-client-wsl-path-invalid", path = path(file)),
            Self::PathLimit => fl!("error-client-wsl-path-limit"),
            Self::PathResponse => fl!("error-client-wsl-path-response"),
            Self::PathUnmapped(file) => {
                fl!("error-client-wsl-path-unmapped", path = path(file))
            }
            Self::Timeout => fl!("error-client-wsl-timeout"),
            Self::Unsupported => fl!("error-client-wsl-unsupported"),
            // keep-sorted end
        }
    }
}
