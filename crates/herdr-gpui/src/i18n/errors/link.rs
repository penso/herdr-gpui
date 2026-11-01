//! Folders a link could not open, from `error-link.ftl`.

use super::Localize;
use crate::{i18n::fl, link::follow::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Unreadable { path, .. } => {
                fl!("error-link-unreadable", path = path.display().to_string())
            }
            Self::NotFolder(path) => {
                fl!("error-link-not-folder", path = path.display().to_string())
            }
            Self::NotCheckout(path) => {
                fl!("error-link-not-checkout", path = path.display().to_string())
            }
            #[cfg(windows)]
            Self::Remote(path) => fl!("error-link-remote", path = path.display().to_string()),
        }
    }
}
