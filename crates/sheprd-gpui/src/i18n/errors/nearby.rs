//! The nearby Bluetooth relay: `error-nearby.ftl`.

use super::Localize;
use crate::{i18n::fl, remote::nearby::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Malformed(_) => fl!("error-nearby-malformed"),
            Self::Pipe(pipe) => fl!("error-nearby-pipe", pipe = *pipe),
            Self::Spawn(_) => fl!("error-nearby-spawn"),
            Self::TooLarge { limit } => fl!("error-nearby-too-large", limit = *limit),
            // keep-sorted end
        }
    }
}
