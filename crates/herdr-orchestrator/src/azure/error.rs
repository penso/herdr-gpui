//! Why Azure DevOps could not be read.

/// Why an Azure DevOps listing failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "Azure DevOps rejected the token; sign in again with `az login`, or replace AZURE_DEVOPS_EXT_PAT"
    )]
    Authentication,
    #[error("Azure DevOps project address for {0:?} is not valid")]
    Address(String),
    #[error(
        "The token cannot read {repository}; it needs the Code (Read) and Work Items (Read) scopes"
    )]
    Forbidden { repository: String },
    #[error("Azure DevOps's answer is not the expected JSON: {0}")]
    Json(#[source] serde_json::Error),
    #[error("Azure DevOps could not be reached at {origin}: {source}")]
    Network {
        /// The organization's address, so a wrong one shows.
        origin: String,
        #[source]
        source: Box<ureq::Error>,
    },
    #[error("Azure DevOps has no repository {repository}, or the token cannot see it")]
    NotFound { repository: String },
    #[error("Azure DevOps rate limit reached; it is read again later")]
    RateLimit,
    #[error("Azure DevOps's answer could not be read: {0}")]
    Read(#[source] std::io::Error),
    #[error(
        "Sign in to read {repository}: run `az login`, or set AZURE_DEVOPS_EXT_PAT in your shell"
    )]
    SignIn { repository: String },
    #[error("Azure DevOps's answer exceeded the size limit")]
    Size,
    #[error("Azure DevOps request failed (HTTP {0})")]
    Status(u16),
    #[error("The Azure DevOps token is not one an HTTP header can carry")]
    Token,
}

impl Error {
    /// A failure only the user can fix, which waits longer before the next
    /// try rather than asking Azure DevOps every minute.
    pub fn needs_user(&self) -> bool {
        matches!(
            self,
            Self::Authentication
                | Self::Forbidden { .. }
                | Self::NotFound { .. }
                | Self::SignIn { .. }
        )
    }
}
