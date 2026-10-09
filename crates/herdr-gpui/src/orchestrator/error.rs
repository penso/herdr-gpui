//! Orchestrator failures keep their typed causes and the path or column they
//! concern until they are shown.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("No data directory is available for orchestrator state")]
    NoDataDirectory,
    #[error("Could not prepare {}: {source}", path.display())]
    Prepare {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{} is not a private file or directory owned by this user", path.display())]
    NotPrivate { path: PathBuf },
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
}

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;
