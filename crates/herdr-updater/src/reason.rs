//! Why App Updates shows no update: the reasons a copy never checks, and the
//! failures an operation ends with. Both stay typed until the app words
//! them in the user's language.
use super::Error;
use std::{fmt, ops::Deref, sync::Arc};

/// Why this copy does not update itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disabled {
    /// A local build or test fixture: no release version or update key.
    LocalBuild,
    /// No standalone updater exists for this platform.
    Platform,
    /// Another window of this process runs the update worker.
    Secondary,
    /// This copy cannot update itself, or its update worker did not start.
    Unsupported(Failure),
}

impl fmt::Display for Disabled {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalBuild => formatter.write_str(
                "In-app updates require a release build with an embedded update verification key. Local builds and test fixtures never check automatically.",
            ),
            Self::Platform => formatter.write_str(
                "No standalone updater is available for this platform. Use your package manager or download a supported release.",
            ),
            Self::Secondary => formatter.write_str(
                "another window owns app updates in this process. Use the window that was open first.",
            ),
            Self::Unsupported(failure) => failure.fmt(formatter),
        }
    }
}

/// A failure App Updates shows. Shared, because the UI clones the state it
/// polls; two states hold the same failure only when they hold this one, so
/// a new failure always counts as a change, even with the same words.
#[derive(Clone, Debug)]
pub struct Failure(Arc<Error>);

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self(Arc::new(error))
    }
}

impl Deref for Failure {
    type Target = Error;

    fn deref(&self) -> &Error {
        &self.0
    }
}

impl PartialEq for Failure {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Failure {}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}
