//! Why Azure DevOps could not be read, or a token could not be saved.

/// Why an Azure DevOps listing, check, or saved token failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "Azure DevOps rejected the token; replace it from the account menu, sign in again with `az login`, or replace AZURE_DEVOPS_EXT_PAT"
    )]
    Authentication,
    #[error("Azure DevOps project address for {0:?} is not valid")]
    Address(String),
    #[error("The folder for saved Azure DevOps tokens could not be found: {0}")]
    Directory(#[source] herdr_client::local_path::PathError),
    #[error(
        "Azure DevOps refused AZURE_DEVOPS_EXT_PAT; change or unset it, since it is tried before a token saved in the app"
    )]
    EnvironmentRefused,
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
    #[cfg(target_os = "linux")]
    #[error(
        "No desktop keyring is running to save an Azure DevOps token in. Start GNOME Keyring, KWallet, or KeePassXC, set [azure_devops] allow_plaintext_credentials = true, or use AZURE_DEVOPS_EXT_PAT"
    )]
    NoKeyring(#[source] herdr_credentials::Error),
    #[error(
        "No secure credential store is available to save an Azure DevOps token in. Set [azure_devops] allow_plaintext_credentials = true, or use AZURE_DEVOPS_EXT_PAT"
    )]
    NoStore(#[source] herdr_credentials::Error),
    #[error("Azure DevOps has no organization {0}, or the token cannot see it")]
    NoOrganization(String),
    #[error("Azure DevOps has no repository {repository}, or the token cannot see it")]
    NotFound { repository: String },
    #[error("{0:?} is not an Azure DevOps organization name")]
    Organization(String),
    #[error("Azure DevOps rate limit reached; it is read again later")]
    RateLimit,
    #[error("Azure DevOps's answer could not be read: {0}")]
    Read(#[source] std::io::Error),
    #[error("The saved Azure DevOps tokens are not in the expected form")]
    Record,
    #[error(
        "Connect Azure DevOps from the account menu to read {repository}, set AZURE_DEVOPS_EXT_PAT, or run `az login`"
    )]
    SignIn { repository: String },
    #[error("Azure DevOps's answer exceeded the size limit")]
    Size,
    #[error("Azure DevOps request failed (HTTP {0})")]
    Status(u16),
    #[error("The Azure DevOps token could not be read or saved: {0}")]
    Store(#[source] herdr_credentials::Error),
    #[error("The Azure DevOps token is not one an HTTP header can carry")]
    Token,
    #[error(
        "Azure DevOps tokens are saved for at most {} organizations",
        super::saved::MAX_ORGANIZATIONS
    )]
    TooManyOrganizations,
}

impl From<herdr_credentials::Error> for Error {
    fn from(error: herdr_credentials::Error) -> Self {
        match error {
            #[cfg(target_os = "linux")]
            herdr_credentials::Error::SecretServiceUnavailable => Self::NoKeyring(error),
            herdr_credentials::Error::Policy => Self::NoStore(error),
            error => Self::Store(error),
        }
    }
}

impl Error {
    /// A failure only the user can fix, which waits longer before the next
    /// try rather than asking Azure DevOps every minute.
    pub fn needs_user(&self) -> bool {
        matches!(
            self,
            Self::Authentication
                | Self::EnvironmentRefused
                | Self::Forbidden { .. }
                | Self::NotFound { .. }
                | Self::SignIn { .. }
        )
    }

    /// A missing or refused token, which a token saved from the account
    /// menu fixes; the view offers that page rather than a failure.
    pub fn needs_sign_in(&self) -> bool {
        matches!(self, Self::SignIn { .. } | Self::Authentication)
    }

    /// The same failure for a token from `source`: a refused
    /// `AZURE_DEVOPS_EXT_PAT` asks for the shell that sets it to change,
    /// since a token saved in the app is only tried after it.
    pub fn from_source(self, source: super::token::Source) -> Self {
        match (self, source) {
            (Self::Authentication, super::token::Source::Environment) => Self::EnvironmentRefused,
            (error, _) => error,
        }
    }
}
