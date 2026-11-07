//! Launch at login's failures on Windows, from `error-launch-at-login.ftl`.

use super::Localize;
use crate::{i18n::fl, launch_at_login::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::NoLauncher { path } => fl!(
                "error-launch-at-login-no-launcher",
                path = path.display().to_string()
            ),
            Self::CurrentExe(_) => fl!("error-launch-at-login-current-exe"),
            Self::Registry { operation, .. } => {
                fl!("error-launch-at-login-registry", operation = *operation)
            }
        }
    }
}
