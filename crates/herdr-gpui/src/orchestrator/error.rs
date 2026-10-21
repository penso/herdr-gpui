//! Orchestrator failures keep their typed causes and the path or column they
//! concern until they are shown.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No state directory is available for orchestrator state")]
    NoDataDirectory,
    #[error(transparent)]
    Private(#[from] crate::private_file::Error),
    #[error("Orchestrator database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Stored {column} is not valid JSON: {source}")]
    Json {
        column: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("Stored {column} is not a valid time: {value:?}")]
    Time {
        column: &'static str,
        value: String,
        #[source]
        source: chrono::ParseError,
    },
    #[error("Unknown issue provider {0:?}")]
    Provider(String),
    #[error("Not an item key: {0:?}")]
    ItemKey(String),
    #[error("Unknown run owner {0:?}")]
    Owner(String),
    #[error(
        "Shared data is schema {found}; this herdr-gpui understands up to {known}. Read-only until updated."
    )]
    NewerSchema { found: i64, known: i64 },
    #[error("Run {0} belongs to another application")]
    NotOwner(String),
    #[error("Run {0} was not found")]
    RunNotFound(String),
    #[error("{operation} failed: {source}")]
    Script {
        operation: &'static str,
        #[source]
        source: herdr_client::Error,
    },
    /// Reading a local repository with gix.
    #[error("{operation} failed: {source}")]
    Git {
        operation: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("{operation} returned unexpected output")]
    Output { operation: &'static str },
    #[error("{operation} returned invalid JSON: {source}")]
    OutputJson {
        operation: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("This host cannot run the orchestrator's scripts")]
    UnsupportedHost,
    #[error("A saved Orchestrator tab names a folder that is not an absolute path")]
    InvalidCheckout,
    #[error("This folder is not inside a Git repository")]
    NotARepository,
    #[error("Remote URL {0:?} names no repository")]
    RemoteUrl(String),
    #[error("Beads is not installed on this host (bd was not found)")]
    BeadsMissing,
    #[error("Bead id {0:?} is not one bd accepts")]
    BeadId(String),
    /// Shown as GitHub's own failure, which already names GitHub and why.
    #[error("{0}")]
    Github(#[source] Box<crate::Error>),
    #[error(transparent)]
    Gitlab(#[from] super::gitlab::Error),
    #[error("Cancelled")]
    Cancelled,
    #[error("Could not start the orchestrator's worker: {0}")]
    Worker(#[source] std::io::Error),
    #[error("Several actions are already running; try again when one finishes")]
    Busy,
    #[error("This run has no Herdr workspace to show")]
    NothingToOpen,
    #[error("This run's workspace is closed")]
    WorkspaceClosed,
    #[error("Close the open menu first")]
    MenuOpen,
    #[error("Only web addresses open here")]
    NotWebAddress,
    #[error("Show this repository's host to dispatch elsewhere")]
    HostNotShown,
    #[error("That host cannot take this repository")]
    HostCannotTake,
    #[error("Setting up the other host failed: {0}")]
    Teleport(#[source] crate::teleport::Error),
    #[error("The pull request changed since you confirmed; look at it again before merging")]
    PullRequestChanged,
}

/// `map_err` adapter naming the host script that failed.
pub(crate) fn script(operation: &'static str) -> impl FnOnce(herdr_client::Error) -> Error {
    move |source| match source {
        herdr_client::Error::ScriptCancelled => Error::Cancelled,
        source => Error::Script { operation, source },
    }
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;
