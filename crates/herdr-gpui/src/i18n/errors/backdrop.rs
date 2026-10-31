//! The window's background picture: `error-backdrop.ftl`.

use super::Localize;
use crate::{backdrop::Error, i18n::fl};

/// `backdrop`'s private file size limit, which `TooLarge` names; the test
/// against its `Display` keeps the two equal.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            // The decoder's and the system's own words.
            Self::Decode { path, source } => fl!(
                "error-backdrop-decode",
                path = path.display().to_string(),
                error = source.to_string()
            ),
            Self::EmptyImage => fl!("error-backdrop-empty-image"),
            Self::ImagePathEncoding => fl!("error-backdrop-image-path-encoding"),
            Self::NotAFile { path } => {
                fl!(
                    "error-backdrop-not-a-file",
                    path = path.display().to_string()
                )
            }
            Self::Opacity => fl!("error-backdrop-opacity"),
            Self::Read { path, source } => fl!(
                "error-backdrop-read",
                path = path.display().to_string(),
                error = source.to_string()
            ),
            Self::RelativeImage => fl!("error-backdrop-relative-image"),
            Self::TooLarge { path } => fl!(
                "error-backdrop-too-large",
                path = path.display().to_string(),
                limit = MAX_FILE_BYTES
            ),
            // keep-sorted end
        }
    }
}
