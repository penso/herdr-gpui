//! `herdr_client::terminal_attach::Error`, a direct terminal attach, from
//! `error-client-terminal-attach.ftl`.

use super::Localize;
use crate::i18n::fl;
use herdr_client::terminal_attach::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Busy => fl!("error-client-terminal-attach-busy"),
            Self::Ended => fl!("error-client-terminal-attach-ended"),
            Self::Io(_) => fl!("error-client-terminal-attach-io"),
            Self::Protocol(_) => fl!("error-client-terminal-attach-protocol"),
            Self::Refused(None) => fl!("error-client-terminal-attach-refused"),
            // Herdr's own bounded words for why.
            Self::Refused(Some(reason)) => fl!(
                "error-client-terminal-attach-refused-reason",
                reason = reason.as_str()
            ),
            Self::Rejected { version, .. } => {
                fl!("error-client-terminal-attach-rejected", version = *version)
            }
            Self::Scroll => fl!("error-client-terminal-attach-scroll"),
            Self::Size => fl!("error-client-terminal-attach-size"),
            Self::Unexpected => fl!("error-client-terminal-attach-unexpected"),
            // keep-sorted end
        }
    }
}
