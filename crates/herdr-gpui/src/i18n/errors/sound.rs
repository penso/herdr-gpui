//! Notification sound failures, from `error-sound.ftl`. The audio library's
//! and the system's text stays as it is.

use super::Localize;
use crate::{i18n::fl, sound::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Cancelled => fl!("error-sound-cancelled"),
            Self::ConfigSize => fl!("error-sound-config-size"),
            Self::Decode(error) => fl!("error-sound-decode", error = error.to_string()),
            Self::Device(error) => fl!("error-sound-device", error = error.to_string()),
            Self::File { path, source } => fl!(
                "error-sound-file",
                path = path.display().to_string(),
                error = source.to_string()
            ),
            Self::FileSize => fl!("error-sound-file-size"),
            Self::Stream(error) => fl!("error-sound-stream", error = error.to_string()),
            Self::Timeout => fl!("error-sound-timeout"),
        }
    }
}
