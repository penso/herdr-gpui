//! Listening ports' and their SSH tunnels' failures, from
//! `error-listening-ports.ftl`.

use super::Localize;
use crate::{
    i18n::fl,
    listening_ports::{Error, tunnel},
};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Scan(_) => fl!("error-listening-ports-scan"),
            Self::Tool => fl!("error-listening-ports-tool"),
            Self::Unsupported => fl!("error-listening-ports-unsupported"),
        }
    }
}

impl Localize for tunnel::Error {
    fn localize(&self) -> String {
        match self {
            Self::Exited(status) => fl!(
                "error-listening-ports-tunnel-exited",
                status = status.to_string()
            ),
            Self::Port(_) => fl!("error-listening-ports-tunnel-port"),
            Self::Start(_) => fl!("error-listening-ports-tunnel-start"),
            Self::Timeout => fl!("error-listening-ports-tunnel-timeout"),
        }
    }
}
