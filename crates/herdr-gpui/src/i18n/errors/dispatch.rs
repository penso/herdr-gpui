//! Sending work to another host: `error-dispatch.ftl`.

use super::Localize;
use crate::{dispatch::Error, i18n::fl};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Busy => fl!("error-dispatch-busy"),
            Self::HostUnavailable(host) => {
                fl!("error-dispatch-host-unavailable", host = host.as_str())
            }
            Self::InvalidHistory => fl!("error-dispatch-invalid-history"),
            // keep-sorted end
        }
    }
}
