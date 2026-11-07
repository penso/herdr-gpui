//! px0 workbench failures, from `error-px0.ftl`. The system's, the HTTP
//! client's, and the JSON parser's text stays as it is.

use super::Localize;
use crate::{i18n::fl, px0::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Address => fl!("error-px0-address"),
            Self::Missing => fl!("error-px0-missing"),
            Self::StateDirectory => fl!("error-px0-state-directory"),
            Self::Discovery(error) => fl!("error-px0-discovery", error = error.to_string()),
            Self::Spawn(error) => fl!("error-px0-spawn", error = error.to_string()),
            Self::Worker(error) => fl!("error-px0-worker", error = error.to_string()),
            Self::Http(error) => fl!("error-px0-http", error = error.to_string()),
            Self::Read(error) => fl!("error-px0-read", error = error.to_string()),
            Self::Json(error) => fl!("error-px0-json", error = error.to_string()),
            Self::NotWorkbench => fl!("error-px0-not-workbench"),
            Self::Timeout => fl!("error-px0-timeout"),
            Self::Exited => fl!("error-px0-exited"),
            Self::Process(error) => fl!("error-px0-process", error = error.to_string()),
            Self::Shutdown(error) => fl!("error-px0-shutdown", error = error.to_string()),
        }
    }
}
