//! Native GitHub transport and device authorization. The one credential
//! subprocess is `gh auth token`, run only to offer the GitHub CLI's sign-in
//! and, once the user chooses it, to read that sign-in again.

mod auth;
mod cli;
#[cfg(windows)]
mod credential_manager;
mod credentials;
mod device;
mod http;
mod log;
mod routing;
#[cfg(target_os = "linux")]
mod secret_service;
mod store;
mod token;

#[cfg(windows)]
pub use credential_manager::Error as CredentialManagerError;
#[cfg(all(unix, feature = "boat"))]
pub(crate) use credentials::read_checked as read_private_entry;

#[cfg(test)]
mod tests;

pub(crate) use {
    auth::Auth,
    device::{Profile, Source, VERIFY_URL},
    http::{graphql, graphql_aliases, hidden_hint, mutation},
    routing::Tokens,
    store::{Account, Note, Store},
};
// Cloud providers keep their own secrets in the same store.
#[cfg(any(
    feature = "azure",
    feature = "boat",
    feature = "coder",
    feature = "daytona"
))]
pub(crate) use store::{Entry, read_entry, save_entry};

use crate::Result;
use device::{Device, Reply, SETUP_MESSAGE, profile, profile_from, token_reply};
use store::{save, valid_token};

/// Delete one account's saved credential, as signing it out does, for an
/// account whose panel is going away with its device. Blocks on the store:
/// call it from a background thread.
pub(crate) fn forget(store: Store, account: &Account) -> Result<()> {
    save(None, store, account)
}
