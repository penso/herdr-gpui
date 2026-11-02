//! Sprites failures keep their category and cause until the UI shows them.
//! Remote text is a bounded diagnostic payload, never a credential.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} must be https (http only on loopback) with no credentials, query, or fragment.")]
    Url(&'static str),
    #[error("{0} is not valid.")]
    Field(crate::cloud::Value),
    #[error("{0} must be UTF-8.")]
    Encoding(crate::cloud::Value),
    #[error("Sprites needs an API token. Save one in Settings > Cloud devices.")]
    MissingToken,
    #[error("Sprites rejected the API token.")]
    Authentication,
    #[error("Sprites denied access. Check the token's organization.")]
    Forbidden,
    #[error("The sprite {0} no longer exists.")]
    Gone(String),
    #[error("{0}")]
    Status(#[from] Status),
    #[error("Sprites network request failed or timed out.")]
    Network(#[source] ureq::Error),
    #[error("Sprites response could not be read within the size/time limit.")]
    Read(#[source] io::Error),
    #[error("Sprites response exceeded the size limit.")]
    Size,
    #[error("Invalid Sprites JSON response.")]
    Json(#[source] serde_json::Error),
    #[error("Invalid Sprites authorization header.")]
    Header(#[source] ureq::http::header::InvalidHeaderValue),
    #[error(transparent)]
    Socket(#[from] crate::cloud::socket::Error),
    #[error("Could not write the command's output.")]
    Output(#[source] io::Error),
    #[error("Could not find this app's executable to relay through.")]
    Executable(#[source] io::Error),
    #[error("Cancelled.")]
    Cancelled,
    #[error(
        "This device belongs to a Sprites account that is no longer configured in [sprites] api_url."
    )]
    Account,
    #[error("{0}")]
    Bridge(#[source] herdr_client::Error),
    #[error(transparent)]
    Cloud(#[from] crate::cloud::Error),
    #[error("{0}")]
    Storage(#[source] Box<crate::Error>),
}

/// A non-success Sprites reply, with its bounded, control-free `error` text.
#[derive(Debug, thiserror::Error)]
#[error("Sprites request failed (HTTP {code}){}", message.as_deref().map(|m| format!(": {m}")).unwrap_or_default())]
pub struct Status {
    pub code: u16,
    pub message: Option<String>,
}
