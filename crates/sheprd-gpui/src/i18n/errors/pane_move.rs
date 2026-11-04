//! Pane and tab move failures, and Herdr's reasons for leaving a pane
//! where it was, from `error-pane-move.ftl`.

use super::Localize;
use crate::{
    i18n::fl,
    pane_move::{Error, Unchanged},
};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::UnsupportedHost => fl!("error-pane-move-unsupported-host"),
            Self::DevelopmentSession => fl!("error-pane-move-development-session"),
            Self::InvalidId(id) => fl!("error-pane-move-invalid-id", id = format!("{id:?}")),
            Self::Unchanged(reason) => reason.localize(),
            // Herdr's own message, as it wrote it.
            Self::Refused { message, .. } => {
                fl!("error-pane-move-refused", error = message.as_str())
            }
            Self::MissingCli(_) => fl!("error-pane-move-missing-cli"),
            Self::Script(error) => fl!("error-pane-move-script", error = error.localize()),
            Self::Decode(_) => fl!("error-pane-move-decode"),
            Self::PartialTabMove {
                pane_id, source, ..
            } => fl!(
                "error-pane-move-partial-tab-move",
                pane = pane_id.as_str(),
                error = source.localize()
            ),
            Self::Rename { source, .. } => {
                fl!("error-pane-move-rename", error = source.localize())
            }
        }
    }
}

impl Localize for Unchanged {
    fn localize(&self) -> String {
        match self {
            Self::SameTab => fl!("error-pane-move-unchanged-same-tab"),
            Self::ZoomedTab => fl!("error-pane-move-unchanged-zoomed-tab"),
            Self::Other => fl!("error-pane-move-unchanged-other"),
        }
    }
}
