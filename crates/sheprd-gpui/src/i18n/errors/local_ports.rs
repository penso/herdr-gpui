//! This computer's listening ports' failures, from `error-local-ports.ftl`.

use super::Localize;
use crate::{i18n::fl, local_ports::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Busy => fl!("error-local-ports-busy"),
            Self::Scan(_) => fl!("error-local-ports-scan"),
            Self::Stopped => fl!("error-local-ports-stopped"),
            Self::Tool => fl!("error-local-ports-tool"),
            Self::Unsupported => fl!("error-local-ports-unsupported"),
            Self::Worker(_) => fl!("error-local-ports-worker"),
        }
    }
}
