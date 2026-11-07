//! Machine failures, from `error-machine.ftl`.

use super::Localize;
use crate::{i18n::fl, machine::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Cpus { min, max } => fl!("error-machine-cpus", min = *min, max = *max),
            Self::Delete { path, source } => fl!(
                "error-machine-delete",
                path = path.display().to_string(),
                error = source.to_string()
            ),
            Self::Framework(message) => {
                fl!("error-machine-framework", message = message.as_str())
            }
            Self::Hardware => fl!("error-machine-hardware"),
            Self::InvalidDisplay => fl!("error-machine-invalid-display"),
            Self::InvalidName => fl!("error-machine-invalid-name"),
            Self::Memory { min, max } => fl!("error-machine-memory", min = *min, max = *max),
            Self::NoStateDir => fl!("error-machine-no-state-dir"),
            Self::NotInstalled(path) => fl!(
                "error-machine-not-installed",
                path = path.display().to_string()
            ),
            Self::Read { path, source } => fl!(
                "error-machine-read",
                path = path.display().to_string(),
                error = source.to_string()
            ),
            Self::Unsupported => fl!("error-machine-unsupported"),
            Self::Write { path, source } => fl!(
                "error-machine-write",
                path = path.display().to_string(),
                error = source.to_string()
            ),
        }
    }
}
