//! `herdr-github`'s errors reach the app through `Error::GitHub`; this lets
//! app code that raises one of its helper errors use `?` and `.into()`.

use super::Error;
use herdr_github::GitHubAccountsError;

impl From<GitHubAccountsError> for Error {
    fn from(error: GitHubAccountsError) -> Self {
        Self::GitHub(error.into())
    }
}
