//! Port forwarding failures, from `error-port-forward.ftl`.

use super::Localize;
use crate::{i18n::fl, port_forward::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Duplicate(port) => fl!("error-port-forward-duplicate", port = *port),
            Self::Limit(limit) => fl!("error-port-forward-limit", limit = *limit),
            Self::Port => fl!("error-port-forward-port"),
        }
    }
}
