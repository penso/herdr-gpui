//! Opening a file in the editor pane: `error-editor.ftl`.

use super::Localize;
use crate::{editor::Error, i18n::fl};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Busy => fl!("error-editor-busy"),
            Self::Command => fl!("error-editor-command"),
            Self::NoPane => fl!("error-editor-no-pane"),
            Self::Path => fl!("error-editor-path"),
            Self::Remote => fl!("error-editor-remote"),
            Self::RemoteBusy => fl!("error-editor-remote-busy"),
            Self::RemoteFailed => fl!("error-editor-remote-failed"),
            Self::RemoteLaunch(_) => fl!("error-editor-remote-launch"),
            Self::Request(error) => error.localize(),
            Self::Response => fl!("error-editor-response"),
            Self::Unsupported => fl!("error-editor-unsupported"),
            // keep-sorted end
        }
    }
}
