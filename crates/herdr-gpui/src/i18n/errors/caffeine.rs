//! Keeping the machine awake: `error-caffeine.ftl`.

use super::Localize;
use crate::{caffeine::Error, i18n::fl};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            // The system's own words.
            Self::Hold(error) => fl!("error-caffeine-hold", error = error.to_string()),
            Self::NoWorkingAgents => fl!("error-caffeine-no-working-agents"),
            Self::Thread(_) => fl!("error-caffeine-thread"),
            Self::WorkerStopped => fl!("error-caffeine-worker-stopped"),
            // keep-sorted end
        }
    }
}
