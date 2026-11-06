//! The Security section's failed reads, from `error-security.ftl`.

use super::Localize;
use crate::{i18n::fl, security::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Github(error) => error.localize(),
            Self::Json(error) => fl!("error-security-json", error = error.to_string()),
            Self::Output => fl!("error-security-output"),
            Self::Unread => fl!("error-security-unread"),
        }
    }
}
