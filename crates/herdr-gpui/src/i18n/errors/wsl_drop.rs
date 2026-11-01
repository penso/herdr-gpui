//! WSL file drop failures, from `error-wsl-drop.ftl`.

use super::Localize;
use crate::{i18n::fl, window::wsl_drop::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Busy => fl!("error-wsl-drop-busy"),
            Self::TargetGone => fl!("error-wsl-drop-target-gone"),
        }
    }
}
