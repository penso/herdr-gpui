//! `[github]`: the sign-in client, the plaintext opt-in, and the additional
//! accounts the GitHub page adds. An account's token is never here: only the
//! credential store entry it signs in under and the owners it reads.

use super::Error;
use super::{Config, daemon_config_path};
use crate::{
    Result,
    github::{self, AccountsError},
};
use serde::Deserialize;
use std::{env, path::Path};

/// More accounts than anyone keeps apart by repository owner; the bound keeps
/// every window's per-account sign-in checks few.
pub(crate) const MAX_ACCOUNTS: usize = 8;
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
    pub(crate) fn generate() -> Result<Self> {
        let mut bytes = [0_u8; ID_BYTES];
        getrandom::fill(&mut bytes)
            .map_err(|error| github::Error::Accounts(AccountsError::Random(error)))?;
        Ok(Self {
            id: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
            owners: Vec::new(),
        })
    }
}

/// 32 lowercase hexadecimal digits: safe in a keyring account name and a
/// file name, which is where an account's ID ends up.
pub(crate) fn valid_account_id(id: &str) -> bool {
    id.len() == ID_BYTES * 2 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A GitHub user or organization name, enterprise managed users' `_shortcode`
/// suffix included. GitHub stays the authority on the exact shape; this only
/// keeps the config to names that could be one.
pub(crate) fn valid_owner(owner: &str) -> bool {
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

    pub(super) fn client_id_with_override(
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
            return Err(Error::InvalidClientId(source).into());
        }
        Ok(id.map(str::to_owned))
    }

    /// Keeps the accounts and owners this build holds, and names the rest as
    /// ignored keys: a newer build may allow more.
    pub(super) fn limit_accounts(&mut self) -> Vec<String> {
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
    pub(super) fn check_accounts(&self) -> Result<()> {
        Ok(check_accounts(&self.accounts).map_err(github::Error::from)?)
    }
}

pub(super) fn check_accounts(accounts: &[GitHubAccount]) -> std::result::Result<(), AccountsError> {
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

impl Config {
    /// Appends `account` to `[github] accounts` in the local overrides file,
    /// under the config lock so two windows adding at once both land.
    /// Blocks on the file: call it from a background thread.
    pub(crate) fn add_github_account(account: &GitHubAccount) -> Result<()> {
        let daemon = daemon_config_path(herdr_client::local_var);
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::add_github_account_at(&local, &daemon, account)
    }

    /// Removes the account `id` from `[github] accounts`, leaving every other
    /// entry as written. Blocks on the file: call it from a background thread.
    pub(crate) fn remove_github_account(id: &str) -> Result<()> {
        let daemon = daemon_config_path(herdr_client::local_var);
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::remove_github_account_at(&local, &daemon, id)
    }

    pub(super) fn add_github_account_at(
        local: &Path,
        daemon: &Path,
        account: &GitHubAccount,
    ) -> Result<()> {
        if !valid_account_id(&account.id) {
            return Err(github::Error::Accounts(AccountsError::Id).into());
        }
        let mut accounts = written_accounts(local)?;
        if accounts.len() >= MAX_ACCOUNTS {
            return Err(github::Error::Accounts(AccountsError::TooMany(MAX_ACCOUNTS)).into());
        }
        let mut entry = toml_edit::InlineTable::new();
        entry.insert("id", account.id.as_str().into());
        if !account.owners.is_empty() {
            entry.insert(
                "owners",
                toml_edit::Value::Array(account.owners.iter().map(String::as_str).collect()),
            );
        }
        accounts.push(toml_edit::Value::InlineTable(entry));
        Self::edit_key_at(local, daemon, "github.accounts", Some(accounts.into())).map(drop)
    }

    pub(super) fn remove_github_account_at(local: &Path, daemon: &Path, id: &str) -> Result<()> {
        let mut accounts = written_accounts(local)?;
        let before = accounts.len();
        accounts.retain(|entry| {
            entry
                .as_inline_table()
                .and_then(|table| table.get("id"))
                .and_then(toml_edit::Value::as_str)
                != Some(id)
        });
        if accounts.len() == before {
            return Ok(());
        }
        let value = (!accounts.is_empty()).then(|| accounts.into());
        Self::edit_key_at(local, daemon, "github.accounts", value).map(drop)
    }
}

/// `[github] accounts` as the local file writes it, each entry an inline
/// table with every field kept, including ones a newer build added.
fn written_accounts(local: &Path) -> Result<toml_edit::Array> {
    let text =
        std::fs::read_to_string(local).map_err(|error| crate::Error::from(error).at_path(local))?;
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| crate::Error::from(error).at_path(local))?;
    let mut array = toml_edit::Array::new();
    match document
        .get("github")
        .and_then(|github| github.get("accounts"))
    {
        None => {}
        Some(toml_edit::Item::Value(toml_edit::Value::Array(written))) => {
            array.extend(written.iter().cloned());
        }
        // `[[github.accounts]]` tables, written by hand.
        Some(toml_edit::Item::ArrayOfTables(tables)) => {
            for table in tables.iter() {
                array.push(table.clone().into_inline_table());
            }
        }
        Some(_) => return Err(github::Error::Accounts(AccountsError::NotAList).into()),
    }
    array.set_trailing_comma(false);
    Ok(array)
}
