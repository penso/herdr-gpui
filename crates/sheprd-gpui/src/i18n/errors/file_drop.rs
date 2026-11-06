//! Dropped path failures, from `error-file-drop.ftl`.

use super::Localize;
use crate::{i18n::fl, window::file_drop::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Control => fl!("error-file-drop-control"),
            Self::EmptyPath => fl!("error-file-drop-empty-path"),
            Self::Encoding => fl!("error-file-drop-encoding"),
            Self::Size => fl!("error-file-drop-size"),
        }
    }
}
