//! The overview page's failures, from `error-overview.ftl`.

use super::Localize;
use crate::{i18n::fl, overview::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::InvalidMarks => fl!("error-overview-invalid-marks"),
            Self::Json(error) => fl!("error-overview-json", error = error.to_string()),
            Self::ReopenRequest(error) => error.localize(),
            Self::ReopenResponse => fl!("error-overview-reopen-response"),
            Self::ReplyHost(error) => error.localize(),
            Self::ReplyRefused { message, .. } => crate::notifications::safe_text(message, 200),
            Self::ReplyScript(error) => {
                fl!("error-overview-reply-script", error = error.to_string())
            }
        }
    }
}
