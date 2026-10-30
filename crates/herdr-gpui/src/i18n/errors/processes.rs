//! Process list failures, from `error-processes.ftl`.

use super::Localize;
use crate::{i18n::fl, processes::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Answer(_) => fl!("error-processes-answer"),
            Self::Busy => fl!("error-processes-busy"),
            Self::Executable => fl!("error-processes-executable"),
            Self::NoRoot => fl!("error-processes-no-root"),
            Self::Query(_) => fl!("error-processes-query"),
            Self::RootExited => fl!("error-processes-root-exited"),
            Self::Stopped => fl!("error-processes-stopped"),
            Self::Worker(_) => fl!("error-processes-worker"),
        }
    }
}
