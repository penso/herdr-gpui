//! Clipboard failures, from `error-clipboard.ftl`.

use super::Localize;
use crate::{i18n::fl, window::clipboard::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Changed => fl!("error-clipboard-changed"),
            Self::Encoding(_) => fl!("error-clipboard-encoding"),
            // The macOS reader's error lives in a module private to the
            // window, so it cannot be matched from here and shows its
            // English `Display`.
            #[cfg(target_os = "macos")]
            Self::Native(error) => error.to_string(),
            Self::Process(_) => fl!("error-clipboard-process"),
            Self::Size { limit } => fl!("error-clipboard-size", limit = *limit),
            Self::Timeout => fl!("error-clipboard-timeout"),
            Self::Unsupported => fl!("error-clipboard-unsupported"),
        }
    }
}
