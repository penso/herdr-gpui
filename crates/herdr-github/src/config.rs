//! `[github]`: the sign-in client, the plaintext opt-in, and the additional
//! accounts the GitHub page adds. An account's token is never here: only the
//! credential store entry it signs in under and the owners it reads.

use crate::{Error, Result, error::AccountsError};
use serde::Deserialize;
use std::env;

/// More accounts than anyone keeps apart by repository owner; the bound keeps
/// every window's per-account sign-in checks few.
pub const MAX_ACCOUNTS: usize = 8;
/// Owners one account lists besides its login and organizations.
const MAX_OWNERS: usize = 32;
const ID_BYTES: usize = 16;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct GitHubConfig {
    pub oauth_client_id: Option<String>,
    pub allow_plaintext_credentials: bool,
    /// Accounts read for the repositories they cover, in this order; any
    /// other repository is read with the main sign-in.
    pub accounts: Vec<GitHubAccount>,
}

/// An additional GitHub account. `id` names its credential store entry, so it
/// stays the same when the account signs in again; `owners` are repository
/// owners it reads besides its own login and organizations. Fields a newer
/// build adds are ignored and named like any unknown key, and kept on edit.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct GitHubAccount {
    pub id: String,
    #[serde(default)]
    pub owners: Vec<String>,
}

impl GitHubAccount {
    /// A new account with a random ID and no owners of its own yet.
    pub fn generate() -> Result<Self> {
        let mut bytes = [0_u8; ID_BYTES];
        getrandom::fill(&mut bytes).map_err(AccountsError::Random)?;
        Ok(Self {
            id: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
            owners: Vec::new(),
        })
    }
}

/// 32 lowercase hexadecimal digits: safe in a keyring account name and a
/// file name, which is where an account's ID ends up.
pub fn valid_account_id(id: &str) -> bool {
    id.len() == ID_BYTES * 2 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A GitHub user or organization name, enterprise managed users' `_shortcode`
/// suffix included. GitHub stays the authority on the exact shape; this only
/// keeps the config to names that could be one.
pub fn valid_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.len() <= 100
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// Herdr GPUI's own GitHub App, used unless another client is configured.
const DEFAULT_CLIENT_ID: &str = "Iv23liurUcwxPjrdIFYT";

impl GitHubConfig {
    /// Where to install the App sign-in uses, known only for Herdr GPUI's
    /// own: another App's page cannot be found from its client ID.
    pub fn app_install_url(&self) -> Option<&'static str> {
        (self.client_id().ok().flatten().as_deref() == Some(DEFAULT_CLIENT_ID))
            .then_some("https://github.com/apps/herdr-ui/installations/new")
    }

    pub fn client_id(&self) -> Result<Option<String>> {
        self.client_id_with_override(env::var_os("HERDR_GITHUB_OAUTH_CLIENT_ID").as_deref())
    }

    pub fn client_id_with_override(
        &self,
        value: Option<&std::ffi::OsStr>,
    ) -> Result<Option<String>> {
        let (id, source) = match value {
            Some(value) => (
                Some(value.to_str().ok_or(Error::ClientIdEncoding)?),
                "HERDR_GITHUB_OAUTH_CLIENT_ID",
            ),
            None => (
                Some(self.oauth_client_id.as_deref().unwrap_or(DEFAULT_CLIENT_ID)),
                "github.oauth_client_id",
            ),
        };
        if let Some(id) = id
            && (id.is_empty()
                || id.len() > 256
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')))
        {
            return Err(Error::InvalidClientId(source));
        }
        Ok(id.map(str::to_owned))
    }

    /// Keeps the accounts and owners this build holds, and names the rest as
    /// ignored keys: a newer build may allow more.
    pub fn limit_accounts(&mut self) -> Vec<String> {
        let mut ignored: Vec<String> = (MAX_ACCOUNTS..self.accounts.len())
            .map(|index| format!("github.accounts.{index}"))
            .collect();
        self.accounts.truncate(MAX_ACCOUNTS);
        for (index, account) in self.accounts.iter_mut().enumerate() {
            ignored.extend(
                (MAX_OWNERS..account.owners.len())
                    .map(|owner| format!("github.accounts.{index}.owners.{owner}")),
            );
            account.owners.truncate(MAX_OWNERS);
        }
        ignored
    }

    /// Refuses accounts the app could not tell apart or store.
    pub fn check_accounts(&self) -> Result<()> {
        check_accounts(&self.accounts).map_err(Error::from)
    }
}

pub fn check_accounts(accounts: &[GitHubAccount]) -> std::result::Result<(), AccountsError> {
    for (index, account) in accounts.iter().enumerate() {
        if !valid_account_id(&account.id) {
            return Err(AccountsError::Id);
        }
        if accounts[..index].iter().any(|other| other.id == account.id) {
            return Err(AccountsError::DuplicateId(account.id.clone()));
        }
        if let Some(owner) = account.owners.iter().find(|owner| !valid_owner(owner)) {
            return Err(AccountsError::Owner(owner.chars().take(100).collect()));
        }
    }
    Ok(())
}
