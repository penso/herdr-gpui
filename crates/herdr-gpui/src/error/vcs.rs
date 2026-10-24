//! Errors from `herdr-vcs`. A GitHub failure behind a Git operation, such as
//! output over the size limit, is handled as the app's own GitHub error, so
//! callers that match it need not know which crate it came through; the rest
//! stay Git, checkpoint, and code index errors.

use super::Error;
use herdr_vcs::Error as Vcs;

impl From<Vcs> for Error {
    fn from(error: Vcs) -> Self {
        match error {
            Vcs::GitHub(error) => error.into(),
            error => Self::Vcs(error),
        }
    }
}
