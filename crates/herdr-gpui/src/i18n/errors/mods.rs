//! Mod failures, from `error-mods.ftl`.

use super::Localize;
use crate::{i18n::fl, mods::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Busy(name) => fl!("error-mods-busy", name = name.as_str()),
            Self::DisabledName(name) => fl!("error-mods-disabled-name", name = name.as_str()),
            // A mod's own diagnostics, for its author: where in the mod it
            // stopped, with Luau's words, as a parser's are shown.
            Self::Mod(error) => error.to_string(),
            Self::NotRunning(name) => fl!("error-mods-not-running", name = name.as_str()),
            Self::Spawn { name, .. } => fl!("error-mods-spawn", name = name.as_str()),
            Self::TooManyDisabled(limit) => fl!("error-mods-too-many-disabled", limit = *limit),
            // keep-sorted end
        }
    }
}
