//! `herdr_client::clipboard::Error`: a clipboard image upload to the
//! daemon, from `error-client-clipboard.ftl`.

use super::Localize;
use crate::i18n::fl;
use herdr_client::clipboard::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Busy => fl!("error-client-clipboard-busy"),
            Self::Cancelled => fl!("error-client-clipboard-cancelled"),
            Self::InputTarget => fl!("error-client-clipboard-input-target"),
            Self::PreparationTimeout => fl!("error-client-clipboard-preparation-timeout"),
            Self::WriteTimeout => fl!("error-client-clipboard-write-timeout"),
            // keep-sorted end
        }
    }
}
