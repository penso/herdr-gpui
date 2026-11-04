use super::*;
use crate::palette::Error;

#[test]
fn every_palette_variant_has_a_message() {
    assert_localized(&[
        Error::CommandChanged,
        Error::ConnectionNotReady,
        Error::DestinationRemoved,
        Error::HostUnavailable,
        Error::InvalidFrecency,
        Error::LocalChanged,
        Error::NoSession,
        Error::PaneRemoved,
        Error::ProjectClone("fatal: repository not found".into()),
        Error::ProjectCreate {
            source: std::io::Error::other("disk on fire"),
        },
        Error::ProjectExists,
        Error::ProjectLimit,
        Error::ProjectPath,
        Error::ProjectRemoved,
        Error::ProjectResponse,
        Error::ProjectRootMissing,
        Error::ProjectRoots,
        Error::ProjectStateChanged,
        Error::ProjectVariable("CODE_HOME".into()),
        Error::SessionChanged,
        Error::TabRemoved,
        Error::WorkspaceRemoved,
    ]);
}
