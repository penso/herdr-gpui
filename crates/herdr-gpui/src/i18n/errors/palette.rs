//! Palette failures, from `error-palette.ftl`.

use super::Localize;
use crate::{i18n::fl, palette::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::CommandChanged => fl!("error-palette-command-changed"),
            Self::ConnectionNotReady => fl!("error-palette-connection-not-ready"),
            Self::DestinationRemoved => fl!("error-palette-destination-removed"),
            Self::HostUnavailable => fl!("error-palette-host-unavailable"),
            Self::InvalidFrecency => fl!("error-palette-invalid-frecency"),
            Self::LocalChanged => fl!("error-palette-local-changed"),
            Self::NoSession => fl!("error-palette-no-session"),
            Self::PaneRemoved => fl!("error-palette-pane-removed"),
            Self::ProjectClone(output) => {
                fl!("error-palette-project-clone", error = output.as_str())
            }
            Self::ProjectCreate { source } => {
                fl!("error-palette-project-create", error = source.to_string())
            }
            Self::ProjectExists => fl!("error-palette-project-exists"),
            Self::ProjectLimit => fl!("error-palette-project-limit"),
            Self::ProjectPath => fl!("error-palette-project-path"),
            Self::ProjectRemoved => fl!("error-palette-project-removed"),
            Self::ProjectResponse => fl!("error-palette-project-response"),
            Self::ProjectRootMissing => fl!("error-palette-project-root-missing"),
            Self::ProjectRoots => fl!("error-palette-project-roots"),
            Self::ProjectStateChanged => fl!("error-palette-project-state-changed"),
            Self::ProjectVariable(name) => {
                fl!("error-palette-project-variable", variable = name.as_str())
            }
            Self::SessionChanged => fl!("error-palette-session-changed"),
            Self::TabRemoved => fl!("error-palette-tab-removed"),
            Self::WorkspaceRemoved => fl!("error-palette-workspace-removed"),
            // keep-sorted end
        }
    }
}
