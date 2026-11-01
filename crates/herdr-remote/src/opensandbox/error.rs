//! OpenSandbox failures keep their category and cause until the UI shows
//! them. Remote text is a bounded diagnostic payload, never a credential.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} must be https (http only on loopback) with no credentials, query, or fragment.")]
    Url(&'static str),
    #[error("{0} is not valid.")]
    Field(crate::cloud::Value),
    #[error("{0} must be UTF-8.")]
    Encoding(crate::cloud::Value),
    #[error("OpenSandbox needs an API key. Save one in Settings > Cloud devices.")]
    MissingKey,
    #[error("OpenSandbox rejected the API key.")]
    Authentication,
    #[error("OpenSandbox denied access.")]
    Forbidden,
    #[error("The sandbox {0} no longer exists.")]
    Gone(String),
    #[error("The sandbox stopped: {0}")]
    Stopped(String),
    #[error("The sandbox did not start within 5 minutes.")]
    StartTimeout,
    #[error("{0}")]
    Status(#[from] Status),
    #[error("OpenSandbox network request failed or timed out.")]
    Network(#[source] ureq::Error),
    #[error("OpenSandbox response could not be read within the size/time limit.")]
    Read(#[source] io::Error),
    #[error("OpenSandbox response exceeded the size limit.")]
    Size,
    #[error("Invalid OpenSandbox JSON response.")]
    Json(#[source] serde_json::Error),
    #[error("Invalid OpenSandbox request header.")]
    Header(#[source] ureq::http::header::InvalidHeaderValue),
    #[error(transparent)]
    Socket(#[from] crate::cloud::socket::Error),
    #[error("The sandbox's shell refused the command: {0}")]
    Shell(String),
    #[error("Could not write the command's output.")]
    Output(#[source] io::Error),
    #[error("Could not find this app's executable to relay through.")]
    Executable(#[source] io::Error),
    #[error("Cancelled.")]
    Cancelled,
    #[error(
        "This device belongs to an OpenSandbox server that is no longer configured in [opensandbox] api_url."
    )]
    Account,
    #[error("{0}")]
    Bridge(#[source] herdr_client::Error),
    #[error(transparent)]
    Cloud(#[from] crate::cloud::Error),
    #[error("{0}")]
    Storage(#[source] Box<crate::Error>),
}

/// A non-success OpenSandbox reply, with its bounded, control-free message.
#[derive(Debug, thiserror::Error)]
#[error("OpenSandbox request failed (HTTP {code}){}", message.as_deref().map(|m| format!(": {m}")).unwrap_or_default())]
pub struct Status {
    pub code: u16,
    pub message: Option<String>,
}
