//! Why a palette action or project lookup failed.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("This command changed or was removed. Reopen the palette.")]
    CommandChanged,
    #[error("The selected connection is not ready.")]
    ConnectionNotReady,
    #[error("This agent or terminal no longer exists. Reopen the palette.")]
    DestinationRemoved,
    #[error("This host is no longer connected. Reopen the palette.")]
    HostUnavailable,
    #[error("Invalid saved palette history")]
    InvalidFrecency,
    #[error("The local connection changed. Reopen the palette.")]
    LocalChanged,
    #[error("No captured daemon session. Reopen the palette.")]
    NoSession,
    #[error("The original pane no longer exists in its tab. Reopen the palette.")]
    PaneRemoved,
    #[error("git clone failed: {0}")]
    ProjectClone(String),
    #[error("Could not create the project folder: {source}")]
    ProjectCreate {
        #[source]
        source: io::Error,
    },
    #[error("A folder of that name already exists in the project root")]
    ProjectExists,
    #[error("Project discovery reached its directory limit; narrow palette.project_roots")]
    ProjectLimit,
    #[error("Project paths must be absolute UTF-8 paths without control characters")]
    ProjectPath,
    #[error("Project directory is no longer available")]
    ProjectRemoved,
    #[error("Unexpected workspace creation response. Review workspace state before retrying.")]
    ProjectResponse,
    #[error("The first configured project root is not available")]
    ProjectRootMissing,
    #[error("palette.project_roots must contain at most 16 nonempty paths of at most 8192 bytes")]
    ProjectRoots,
    #[error("Workspace directories changed. Select the project again.")]
    ProjectStateChanged,
    #[error("Project path references an unset environment variable: {0}")]
    ProjectVariable(String),
    #[error("The daemon session changed. Reopen the palette.")]
    SessionChanged,
    #[error("The original tab no longer exists in its workspace. Reopen the palette.")]
    TabRemoved,
    #[error("This workspace no longer exists. Reopen the palette.")]
    WorkspaceRemoved,
    // keep-sorted end
}
