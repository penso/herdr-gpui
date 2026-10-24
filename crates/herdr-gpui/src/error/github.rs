//! `herdr-github`'s feature errors reach the app through `Error::GitHub`;
//! these let app code that raises one of them use `?` and `.into()`.

use super::Error;
use herdr_github::{AccountsError, pr_actions, pull_request};

impl From<AccountsError> for Error {
    fn from(error: AccountsError) -> Self {
        Self::GitHub(error.into())
    }
}

impl From<pr_actions::Error> for Error {
    fn from(error: pr_actions::Error) -> Self {
        Self::GitHub(error.into())
    }
}

impl From<pull_request::Error> for Error {
    fn from(error: pull_request::Error) -> Self {
        Self::GitHub(error.into())
    }
}
