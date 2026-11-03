//! Failures tied to the file they concern.
use super::Error;
use std::path::Path;

impl Error {
    /// This failure, as one concerning `path`.
    pub(crate) fn at_path(self, path: &Path) -> Self {
        Self::Path {
            path: path.to_owned(),
            source: Box::new(self),
        }
    }
}
