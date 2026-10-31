//! `herdr_control_protocol::Error`: reaching or serving the app's control
//! socket, from `error-control.ftl`.

use super::Localize;
use crate::i18n::fl;
use herdr_control_protocol::Error;
use std::path::Path;

fn path(path: &Path) -> String {
    path.display().to_string()
}

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            // The system's own words, as `Json` is its parser's.
            Self::Io(error) => error.to_string(),
            Self::Json(error) => error.to_string(),
            Self::MessageSize { limit } => fl!("error-control-message-size", limit = *limit),
            Self::NoResponse => fl!("error-control-no-response"),
            Self::PipeForeign { path: file } => {
                fl!("error-control-pipe-foreign", path = path(file))
            }
            // The running app's own answer, as it worded it.
            Self::Rejected { message, .. } => message.clone(),
            Self::SocketInUse { path: file } => {
                fl!("error-control-socket-in-use", path = path(file))
            }
            Self::Unavailable { path: file, source } => fl!(
                "error-control-unavailable",
                path = path(file),
                error = source.to_string()
            ),
            Self::User(error) => fl!("error-control-user", error = error.to_string()),
            // keep-sorted end
        }
    }
}
