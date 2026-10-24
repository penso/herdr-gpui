//! GitHub transport, sign-in, and `[github]` config failures. Pull request
//! lookups, pull request actions, and the GitHub CLI each keep their own
//! `Error`, wrapped here once.

use std::io;

pub type Result<T, E = Error> = std::result::Result<T, E>;

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
    #[error(transparent)]
    Cli(#[from] crate::cli::Error),
    // Not transparent: the client error stays in the source chain, where
    // `StorageFailure::find` and the chain printers look for it.
    #[error("{0}")]
    Client(#[from] herdr_client::Error),
    #[error("HERDR_GITHUB_OAUTH_CLIENT_ID must be UTF-8")]
    ClientIdEncoding,
    #[error(transparent)]
    Credentials(#[from] herdr_credentials::Error),
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
    #[error(
        "{0} must be 1..256 ASCII letters, digits, '.', '_' or '-' (public client ID, not a secret)"
    )]
    InvalidClientId(&'static str),
    #[error("Invalid GitHub JSON response.")]
    Json(#[source] JsonError),
    #[error("GitHub network request failed or timed out ({0}).")]
    Network(#[source] ureq::Error),
    #[error(transparent)]
    Path(#[from] herdr_client::local_path::PathError),
    #[error(transparent)]
    PrActions(#[from] crate::pr_actions::Error),
    #[error(transparent)]
    PullRequest(#[from] crate::pull_request::Error),
    #[error("GitHub query failed. Check token repository permissions and rate limits.")]
    Query,
    /// The account's (or, signed out, this address's) API quota is used up.
    #[error("GitHub rate limit reached. {}.", herdr_updater::retry_hint(*reset))]
    RateLimit {
        reset: Option<chrono::DateTime<chrono::Utc>>,
    },
    #[error("GitHub response could not be read within the size/time limit.")]
    Read(#[source] io::Error),
    /// GitHub's own refusal of a requested change, cleaned and bounded.
    #[error("GitHub refused the request: {0}")]
    Rejected(String),
    /// GraphQL answers a repository the credential cannot reach as missing.
    #[error("GitHub cannot see {repository} with this sign-in. {}", crate::hidden_hint(*app_token))]
    RepositoryHidden { repository: String, app_token: bool },
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
    pub fn json(source: serde_json::Error) -> Self {
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
