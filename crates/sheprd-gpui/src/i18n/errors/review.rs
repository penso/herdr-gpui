//! Review failures, from `error-review.ftl`.

use super::Localize;
use crate::{i18n::fl, review::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::InvalidViewed => fl!("error-review-invalid-viewed"),
            Self::NoBase => fl!("error-review-no-base"),
            Self::TooLarge => fl!("error-review-too-large"),
        }
    }
}
