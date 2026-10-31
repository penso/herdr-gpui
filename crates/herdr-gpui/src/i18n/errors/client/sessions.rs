//! `herdr_client::sessions::Error`: listing or deleting a machine's named
//! sessions, from `error-client-sessions.ftl`.

use super::Localize;
use crate::i18n::fl;
use herdr_client::sessions::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::DeleteDefault => fl!("error-client-sessions-delete-default"),
            Self::DeleteFailed(status) => fl!(
                "error-client-sessions-delete-failed",
                status = status.to_string()
            ),
            Self::DeleteTimeout => fl!("error-client-sessions-delete-timeout"),
            Self::Limit => fl!("error-client-sessions-limit"),
            // keep-sorted end
        }
    }
}
