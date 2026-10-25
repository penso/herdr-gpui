//! Native GitHub transport and device authorization, the read-only pull
//! request prefetch, and explicit pull request actions. The one credential
//! subprocess is `gh auth token`, run only to offer the GitHub CLI's sign-in
//! and, once the user chooses it, to read that sign-in again. Every request
//! blocks, so callers run it on a worker thread, never the UI thread; the app
//! draws the menus and badges and keeps the account avatars.

#![cfg_attr(test, allow(clippy::disallowed_methods))]

mod auth;
pub mod checkout;
pub mod cli;
pub mod config;
mod credentials;
mod device;
mod error;
mod http;
mod installation;
#[cfg(any(test, feature = "test-support"))]
pub use http::fake_graphql;
mod log;
pub mod pr_actions;
pub mod pull_request;
mod repository;
mod routing;
mod store;
mod token;

#[cfg(all(test, unix))]
#[path = "../../test-support/executable.rs"]
mod test_executable;
#[cfg(test)]
mod tests;

pub use error::{AccountsError, Error, JsonError, Result};
pub use repository::github_repo;
pub use {
    auth::Auth,
    device::{Profile, Source, VERIFY_URL},
    http::{graphql, graphql_aliases, hidden_hint, mutation, rest},
    installation::{Access, Installation},
    routing::Tokens,
    store::{Account, Note, Store, note as store_note, select as select_store},
};

use device::{Device, Reply, SETUP_MESSAGE, profile, profile_from, token_reply};
use store::{save, valid_token};

/// Delete one account's saved credential, as signing it out does, for an
/// account whose panel is going away with its device. Blocks on the store:
/// call it from a background thread.
pub fn forget(store: Store, account: &Account) -> Result<()> {
    save(None, store, account)
}
