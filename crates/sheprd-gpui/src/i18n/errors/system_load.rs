//! CPU and memory reading failures, from `error-system-load.ftl`.

use super::Localize;
use crate::{i18n::fl, system_load::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Output => fl!("error-system-load-output"),
            Self::Remote(_) => fl!("error-system-load-remote"),
            Self::Unsupported(system) => fl!(
                "error-system-load-unsupported",
                system = format!("{system:?}")
            ),
        }
    }
}
