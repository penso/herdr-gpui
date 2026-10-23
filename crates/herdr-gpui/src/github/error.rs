//! Why a GitHub request, sign-in, or credential store operation failed.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Accounts(#[from] AccountsError),
    #[error(
        "GitHub authentication required. Use menu > GitHub sign-in or set GH_TOKEN / GITHUB_TOKEN."
    )]
    Authentication,
    #[error("GitHub authorization failed. Check OAuth application settings.")]
    Authorization,
    #[error("GitHub CLI (gh) was not found. Install it and run `gh auth login`, or sign in here.")]
    CliMissing,
    #[error("Could not run the GitHub CLI (gh).")]
    CliProcess(#[source] Box<crate::Error>),
    #[error(
        "GitHub rejected the GitHub CLI's token. Run `gh auth login` to renew it, then try again."
    )]
    CliRejected,
    #[error("GitHub CLI is not signed in to github.com. Run `gh auth login`, then try again.")]
    CliSignedOut,
    #[error("GitHub CLI did not answer within 10 seconds. Check `gh auth status`, then try again.")]
    CliTimeout,
    #[error("Missing credential directory.")]
    CredentialDirectory,
    #[error("Cannot access private credential file.")]
    CredentialIo(#[source] io::Error),
    #[cfg(windows)]
    #[error(transparent)]
    CredentialManager(#[from] super::credential_manager::Error),
    #[error(
        "Cannot access private credential file. Require an owned directory and regular 0600 file; symlinks are rejected."
    )]
    CredentialPermissions,
    #[error(
        "No secure credential store configured. Explicitly opt in with allow_plaintext_credentials = true in the [github] or [coder] table, or use GH_TOKEN / GITHUB_TOKEN for GitHub."
    )]
    CredentialPolicy,
    #[error(
        "No GitHub credential store is available on this platform. Use GH_TOKEN / GITHUB_TOKEN."
    )]
    CredentialUnsupported,
    #[error("GitHub authorization denied.")]
    Denied,
    #[error("Invalid GitHub device authorization response.")]
    Device,
    #[error("Invalid GitHub credential encoding.")]
    Encoding(#[source] std::str::Utf8Error),
    #[error("GitHub code expired. Sign in again.")]
    Expired,
    #[error("GitHub denied access: check token permissions or SSO authorization.")]
    Forbidden,
    #[error("Invalid GitHub authorization header.")]
    Header(#[source] ureq::http::header::InvalidHeaderValue),
    #[error("Invalid GitHub JSON response.")]
    Json(#[source] JsonError),
    #[cfg(target_os = "macos")]
    #[error("Cannot read GitHub Keychain entry. Unlock your login Keychain or set GH_TOKEN.")]
    KeychainRead(#[source] security_framework::base::Error),
    #[cfg(target_os = "macos")]
    #[error("GitHub Keychain update failed. Unlock your login Keychain and try again.")]
    KeychainWrite(#[source] security_framework::base::Error),
    #[error("GitHub network request failed or timed out ({0}).")]
    Network(#[source] ureq::Error),
    #[error("GitHub query failed. Check token repository permissions and rate limits.")]
    Query,
    /// The account's (or, signed out, this address's) API quota is used up.
    #[error("GitHub rate limit reached. {}.", crate::updater::retry_hint(*reset))]
    RateLimit {
        reset: Option<chrono::DateTime<chrono::Utc>>,
    },
    #[error("GitHub response could not be read within the size/time limit.")]
    Read(#[source] io::Error),
    /// GitHub's own refusal of a requested change, cleaned and bounded.
    #[error("GitHub refused the request: {0}")]
    Rejected(String),
    /// GraphQL answers a repository the credential cannot reach as missing.
    #[error("GitHub cannot see {repository} with this sign-in. {}", crate::github::hidden_hint(*app_token))]
    RepositoryHidden { repository: String, app_token: bool },
    #[error(
        "[github] {0} must not be in the config. Sign in from the app, or use GH_TOKEN / GITHUB_TOKEN."
    )]
    SecretInConfig(&'static str),
    #[cfg(target_os = "linux")]
    #[error(
        "Cannot read GitHub sign-in from the desktop keyring. Unlock your keyring or set GH_TOKEN."
    )]
    SecretServiceRead(#[source] Box<oo7::Error>),
    #[cfg(target_os = "linux")]
    #[error(
        "No desktop keyring (Secret Service) is running. Start GNOME Keyring, KWallet, or KeePassXC, or set [github] allow_plaintext_credentials = true, or use GH_TOKEN / GITHUB_TOKEN."
    )]
    SecretServiceUnavailable,
    #[cfg(target_os = "linux")]
    #[error("GitHub keyring update failed. Unlock your desktop keyring and try again.")]
    SecretServiceWrite(#[source] Box<oo7::Error>),
    #[error("GitHub response exceeded the size limit.")]
    Size,
    #[error("GitHub request failed (HTTP {0}). Check network and repository access.")]
    Status(u16),
    #[error("Invalid GitHub token. Replace the configured credential.")]
    Token,
    #[error("Unsupported GitHub token type.")]
    TokenType,
    #[error("GitHub {0} worker stopped.")]
    Worker(&'static str),
    // keep-sorted end
}

impl Error {
    /// Redacts the parser diagnostic while keeping it as a source.
    pub(crate) fn json(source: serde_json::Error) -> Self {
        Self::Json(JsonError(source))
    }
}

/// Parser diagnostics can quote credential-bearing fields; expose the cause only
/// to explicit source inspection, never ordinary Display or Debug formatting.
#[derive(thiserror::Error)]
#[error("Invalid GitHub JSON response.")]
pub struct JsonError(#[source] serde_json::Error);

impl std::fmt::Debug for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JsonError([REDACTED])")
    }
}

/// Why `[github] accounts`, or a change to it, was refused.
#[derive(Debug, thiserror::Error)]
pub enum AccountsError {
    #[error("[github] accounts may list at most {0} accounts")]
    TooMany(usize),
    #[error("[github] accounts id must be 32 lowercase hexadecimal digits")]
    Id,
    #[error("[github] accounts lists the id {0} twice")]
    DuplicateId(String),
    #[error("[github] accounts must be a list of account tables")]
    NotAList,
    #[error("[github] accounts owner {0:?} is not a GitHub user or organization name")]
    Owner(String),
    #[error("Could not generate a GitHub account ID")]
    Random(#[source] getrandom::Error),
}
