//! Where a GitHub token lives for this build, and the precedence between the
//! environment and that store. A signed macOS release build uses the Keychain,
//! Linux the Secret Service, and Windows Credential Manager; every build that
//! keeps the token unencrypted on disk says so plainly.

use super::{
    Profile, Result, cli, credentials, profile,
    token::{Credential, GITHUB_CLI, Saved},
};
use crate::Error;
pub use herdr_credentials::{FILE, Store};
pub(super) use herdr_credentials::{FILE_DEFAULT, KEYRING};
use herdr_credentials::{OPT_IN, keyring_read, keyring_save};
use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroizing;

// All windows share one credential. A refresh token is single-use, and a late
// renewal must not recreate an entry after another window has deleted it.
// Called only by background workers; never hold this lock on the UI thread.
fn transaction<T>(work: impl FnOnce() -> Result<T>) -> Result<T> {
    static ACCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ACCESS.lock().unwrap_or_else(|error| error.into_inner());
    work()
}

pub const PLAINTEXT_WARNING: &str = "WARNING: plaintext credential storage is enabled. GitHub tokens are unencrypted on disk; software running as you and backups can read them.";
pub const DEVELOPMENT_WARNING: &str = "WARNING: this development build does not use the macOS Keychain. GitHub tokens are unencrypted beside the GUI config; software running as you and backups can read them.";
#[cfg(not(any(target_os = "linux", windows)))]
pub const KEYRING_NOTICE: &str =
    "Credentials are saved in macOS Keychain. macOS may ask you to unlock or approve access.";
#[cfg(target_os = "linux")]
pub const KEYRING_NOTICE: &str =
    "Credentials are saved in your desktop keyring (Secret Service). It may ask you to unlock it.";
#[cfg(windows)]
pub const KEYRING_NOTICE: &str =
    "Credentials are saved in Windows Credential Manager for your Windows account.";

// The GitHub sign-in's keyring entries: one service, an account per sign-in.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
const SERVICE: &str = "dev.herdr.gpui.github";
#[cfg(any(target_os = "linux", windows))]
const LABEL: &str = "Herdr UI GitHub sign-in";

/// Which saved GitHub sign-in a credential belongs to. Each one is a separate
/// entry in the same store, so signing one out never touches another.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Account {
    /// The account used on this device, and on any saved host without its own.
    #[default]
    Main,
    /// A saved SSH device's own account, keyed by its catalog profile ID.
    Host(String),
    /// An additional account, read for the repository owners it covers,
    /// keyed by its `[github] accounts` ID.
    Additional(String),
}

impl Account {
    /// Catalog profile IDs are 32 lowercase hex digits, so they are safe in a
    /// keyring account name and a file name. Anything else is refused.
    pub fn host(id: &str) -> Option<Self> {
        herdr_client::valid_profile_id(id).then(|| Self::Host(id.to_owned()))
    }

    /// The same rule as `host`, for an additional account's config ID.
    pub fn additional(id: &str) -> Option<Self> {
        crate::config::valid_account_id(id).then(|| Self::Additional(id.to_owned()))
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn keyring_account(&self) -> String {
        match self {
            Self::Main => "github.com".into(),
            Self::Host(id) => format!("github.com/host/{id}"),
            Self::Additional(id) => format!("github.com/account/{id}"),
        }
    }

    fn file_name(&self) -> std::ffi::CString {
        let name = match self {
            Self::Main => "github-credentials".to_owned(),
            Self::Host(id) => format!("github-credentials-{id}"),
            Self::Additional(id) => format!("github-credentials-account-{id}"),
        };
        // No form can contain NUL: `host` and `additional` admit only hex digits.
        std::ffi::CString::new(name).unwrap_or_default()
    }
}

pub(super) fn valid_token(token: &str) -> bool {
    !token.is_empty() && token.len() <= 4096 && token.bytes().all(|b| b.is_ascii_graphic())
}

pub(super) fn resolve_token(
    gh: Option<SecretString>,
    github: Option<SecretString>,
    saved: impl FnOnce() -> Result<Option<SecretString>>,
) -> Result<SecretString> {
    let token = match gh
        .filter(|s| !s.expose_secret().trim().is_empty())
        .or_else(|| github.filter(|s| !s.expose_secret().trim().is_empty()))
    {
        Some(token) => Some(token),
        None => saved()?,
    }
    .ok_or(Error::Authentication)?;
    let trimmed = token.expose_secret().trim();
    if !valid_token(trimmed) {
        return Err(Error::Token);
    }
    if trimmed.len() == token.expose_secret().len() {
        Ok(token)
    } else {
        Ok(trimmed.into())
    }
}

pub(super) fn credential_bytes(bytes: Vec<u8>) -> Result<SecretString> {
    let bytes = Zeroizing::new(bytes);
    // Validate by borrowing so invalid UTF-8 never escapes in FromUtf8Error.
    std::str::from_utf8(&bytes)
        .map(SecretString::from)
        .map_err(Error::Encoding)
}

pub(super) fn environment_token(name: &str) -> Result<Option<SecretString>> {
    // Own and wipe even non-Unicode environment values. The process environment
    // itself is outside this allocation's lifetime and is not erased here.
    std::env::var_os(name)
        .map(|value| credential_bytes(value.into_encoded_bytes()))
        .transpose()
}

/// The store for GitHub sign-ins under `config`'s plaintext opt-in.
pub fn select(config: &crate::config::GitHubConfig) -> Store {
    Store::choose(
        config.allow_plaintext_credentials && OPT_IN,
        KEYRING,
        FILE_DEFAULT,
    )
}

/// Credential handling shown in the GitHub menu, so storage is never implicit.
pub fn note(store: Store, connected: bool) -> Option<Note> {
    match store {
        Store::File if FILE_DEFAULT => Some(Note::Warning(DEVELOPMENT_WARNING)),
        Store::File => Some(Note::Warning(PLAINTEXT_WARNING)),
        Store::Keyring if !connected => Some(Note::Info(KEYRING_NOTICE)),
        Store::Keyring | Store::Environment => None,
    }
}

pub enum Note {
    Warning(&'static str),
    Info(&'static str),
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn keyring_token(account: &Account) -> Result<Option<SecretString>> {
    Ok(keyring_read(SERVICE, &account.keyring_account())?)
}

// `Store::Keyring` is never selected elsewhere; the stub keeps the match total.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn keyring_token(_: &Account) -> Result<Option<SecretString>> {
    Ok(None)
}

#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn keyring_save_token(token: Option<&SecretString>, account: &Account) -> Result<()> {
    #[cfg(target_os = "macos")]
    let label = "";
    #[cfg(any(target_os = "linux", windows))]
    let label = LABEL;
    Ok(keyring_save(
        SERVICE,
        label,
        token,
        &account.keyring_account(),
    )?)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn keyring_save_token(_: Option<&SecretString>, _: &Account) -> Result<()> {
    Err(herdr_credentials::Error::Policy.into())
}

pub(super) fn saved_token(store: Store, account: &Account) -> Result<Option<SecretString>> {
    match store {
        Store::Environment => Ok(None),
        Store::Keyring => keyring_token(account),
        Store::File => credentials::read(&credential_directory()?, &account.file_name()),
    }
}

/// The account's profile: the environment's token first for the main
/// account, then what the store saved. `github_cli` is the user's choice of
/// the GitHub CLI in this app session; it stands in for the saved marker only
/// where there is no store to keep one.
pub(super) fn load_profile(
    store: Store,
    account: &Account,
    github_cli: bool,
) -> Result<Option<Profile>> {
    // The environment names one account for this device. A host's own sign-in
    // exists precisely to differ from it, so only the main account reads it.
    if *account != Account::Main {
        return restore(store, account, false, cli::profile);
    }
    let gh = environment_token("GH_TOKEN")?;
    let github = if gh
        .as_ref()
        .is_none_or(|s| s.expose_secret().trim().is_empty())
    {
        environment_token("GITHUB_TOKEN")?
    } else {
        None
    };
    if gh
        .as_ref()
        .is_none_or(|s| s.expose_secret().trim().is_empty())
        && github
            .as_ref()
            .is_none_or(|s| s.expose_secret().trim().is_empty())
    {
        return restore(store, account, github_cli, cli::profile);
    }
    tracing::info!(
        category = "github_restore",
        "Checking environment GitHub token"
    );
    let token = resolve_token(gh, github, || Ok(None))?;
    profile(std::sync::Arc::new(token)).map(Some)
}

/// What the store held, once its own sign-in is loaded under the lock.
enum Restored {
    Profile(Option<Profile>),
    /// The main account uses the GitHub CLI, read once the lock is released.
    GitHubCli,
}

/// Read the saved sign-in under the lock, then the GitHub CLI outside it: a
/// slow `gh` or GitHub must not hold up other windows' and accounts' loads,
/// saves, and sign-outs. Nothing is written afterwards, and the account
/// discards a result that lands after its own sign-out or reload.
pub(super) fn restore(
    store: Store,
    account: &Account,
    github_cli: bool,
    read_cli: impl FnOnce() -> Result<Profile>,
) -> Result<Option<Profile>> {
    match transaction(|| load_saved(store, account, github_cli))? {
        Restored::Profile(profile) => Ok(profile),
        Restored::GitHubCli => read_cli().map(Some),
    }
}

/// Call only inside `transaction`: a renewal rotates the single-use refresh token.
fn load_saved(store: Store, account: &Account, github_cli: bool) -> Result<Restored> {
    let saved = match store {
        Store::Environment if github_cli => Some(Saved::GitHubCli),
        store => saved_token(store, account)?
            .map(|token| Saved::decode(&token))
            .transpose()?,
    };
    tracing::info!(
        category = "github_restore",
        ?store,
        host = matches!(account, Account::Host(_)),
        present = saved.is_some(),
        github_cli = matches!(saved, Some(Saved::GitHubCli)),
        "Checking saved GitHub sign-in"
    );
    match saved {
        None => Ok(Restored::Profile(None)),
        // The local GitHub CLI is this computer's account. A host's own
        // sign-in exists to differ from it, so only the main one uses it.
        Some(Saved::GitHubCli) if *account == Account::Main => Ok(Restored::GitHubCli),
        Some(Saved::GitHubCli) => Err(Error::Token),
        Some(Saved::Token(credential)) => credential
            .profile_with(profile, Credential::refresh, |value| {
                save_unlocked(Some(value), store, account)
            })
            .map(|profile| Restored::Profile(Some(profile))),
    }
}

/// Remember that the main account uses the GitHub CLI. Without a store the
/// choice lasts for this app session only, so there is nothing to save.
pub(super) fn save_github_cli(store: Store) -> Result<()> {
    if store == Store::Environment {
        return Ok(());
    }
    save(Some(&GITHUB_CLI.into()), store, &Account::Main)
}

/// The directory the GUI config, and so the private credential file, lives in.
pub(super) fn credential_directory() -> Result<std::path::PathBuf> {
    Ok(herdr_client::local_path::config_root()?.join("herdr"))
}

pub(super) fn save(token: Option<&SecretString>, store: Store, account: &Account) -> Result<()> {
    transaction(|| save_unlocked(token, store, account))
}

fn save_unlocked(token: Option<&SecretString>, store: Store, account: &Account) -> Result<()> {
    let name = account.file_name();
    match store {
        Store::Keyring => keyring_save_token(token, account),
        // Removal stays allowed without an opt-in, so a file written under an
        // earlier policy is still cleaned up by an explicit sign-out.
        Store::Environment => credentials::store(&credential_directory()?, &name, token, false),
        Store::File => credentials::store(&credential_directory()?, &name, token, true),
    }
}

#[cfg(test)]
mod tests;
