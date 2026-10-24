//! `[github]` in the config files: adding and removing the additional
//! accounts the GitHub page lists. The table itself lives in `herdr-github`.

use super::{Config, daemon_config_path};
use crate::{Error, Result};
use herdr_github::AccountsError;
pub(crate) use herdr_github::config::MAX_ACCOUNTS;
use herdr_github::config::valid_account_id;
pub use herdr_github::config::{GitHubAccount, GitHubConfig};
use std::path::Path;

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
            return Err(AccountsError::Id.into());
        }
        let mut accounts = written_accounts(local)?;
        if accounts.len() >= MAX_ACCOUNTS {
            return Err(AccountsError::TooMany(MAX_ACCOUNTS).into());
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
    let text = std::fs::read_to_string(local).map_err(|error| Error::from(error).at_path(local))?;
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| Error::from(error).at_path(local))?;
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
        Some(_) => return Err(AccountsError::NotAList.into()),
    }
    array.set_trailing_comma(false);
    Ok(array)
}
