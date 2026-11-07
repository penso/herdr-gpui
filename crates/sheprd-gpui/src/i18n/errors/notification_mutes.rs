//! `notification_mutes::Error`, from `error-notification-mutes.ftl`.

use super::Localize;
use crate::{i18n::fl, notification_mutes::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Invalid => fl!("error-notification-mutes-invalid"),
            Self::Json(error) => fl!("error-notification-mutes-json", error = error.to_string()),
        }
    }
}
