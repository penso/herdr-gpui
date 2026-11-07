//! Device setup relay failures, from `error-setup-relay.ftl`.

use super::Localize;
use crate::{i18n::fl, setup_relay::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Closed => fl!("error-setup-relay-closed"),
            Self::Connect(_) => fl!("error-setup-relay-connect"),
            Self::Dir(path) => fl!("error-setup-relay-dir", path = path.display().to_string()),
            Self::Frame => fl!("error-setup-relay-frame"),
            Self::Terminal => fl!("error-setup-relay-terminal"),
        }
    }
}
