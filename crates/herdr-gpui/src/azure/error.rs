//! Azure failures keep their category and cause until the UI displays them.
//! Remote text is bounded diagnostic payload, never a credential.

use std::io;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} is not valid.")]
    Field(&'static str),
    #[error("{0} must be UTF-8.")]
    Encoding(&'static str),
    #[error("Azure needs a subscription ID. Configure one in Settings > Cloud Devices.")]
    MissingSubscription,
    #[error("Azure needs credentials (client secret or Azure CLI login).")]
    MissingCredentials,
    #[error("Azure authentication failed: {0}")]
    Authentication(String),
    #[error("Azure denied access: {0}")]
    Forbidden(String),
    #[error("Azure API returned HTTP {0}: {1}")]
    Status(u16, String),
    #[error("Azure network request failed or timed out: {0}")]
    Network(#[source] ureq::Error),
    #[error("Azure response could not be read within the size/time limit.")]
    Read(#[source] io::Error),
    #[error("Azure response exceeded the size limit.")]
    Size,
    #[error("Invalid Azure JSON response.")]
    Json(#[source] serde_json::Error),
    #[error("Invalid Azure HTTP header.")]
    Header(#[source] ureq::http::header::InvalidHeaderValue),
    #[error("Azure VM failed: {0}")]
    Vm(String),
    #[error("The Azure VM was deleted.")]
    Deleted,
    #[error("The Azure VM did not become ready within 10 minutes.")]
    StartTimeout,
    #[error("Azure sandbox failed: {0}")]
    Sandbox(String),
    #[error("The Azure sandbox was deleted.")]
    SandboxDeleted,
    #[error("The Azure sandbox did not become ready within 5 minutes.")]
    SandboxTimeout,
    #[error("Azure did not finish deleting the machine within 10 minutes.")]
    DeleteTimeout,
    #[error("The Azure sandbox shell stream failed.")]
    Stream(#[source] tungstenite::Error),
    #[error("Invalid Azure sandbox shell stream header.")]
    StreamHeader(#[source] tungstenite::http::header::InvalidHeaderValue),
    #[error("The Azure sandbox shell stream closed before the command finished.")]
    StreamClosed,
    #[error("The Azure sandbox shell stream sent data this app does not understand.")]
    StreamMessage,
    #[error("Could not set up TLS for the Azure sandbox shell stream.")]
    Tls(#[source] rustls::Error),
    #[error("Could not find this app's executable to relay the sandbox shell.")]
    Executable(#[source] io::Error),
    #[error("Cancelled.")]
    Cancelled,
    #[error("Azure returned SSH connection data this app does not understand.")]
    Gateway,
    #[error("Azure devices need ssh and ssh-keygen on the PATH; one could not be run.")]
    SshMissing(#[source] io::Error),
    #[error("Generating the Azure device SSH key failed.")]
    SshKeygen,
    #[error("Azure CLI (az) was not found on PATH.")]
    AzMissing,
    #[error("Azure CLI command failed: {0}")]
    AzFailed(String),
    #[error("Could not run the command that reaches the machine.")]
    Process(#[source] io::Error),
    #[error(transparent)]
    Bridge(herdr_client::Error),
    #[error("Could not update saved Azure credentials: {0}")]
    Storage(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error(transparent)]
    Cloud(#[from] crate::cloud::Error),
    #[error("Could not save cloud device {id}: {source}")]
    SaveDevice {
        id: String,
        #[source]
        source: Box<crate::Error>,
    },
    #[error("The Azure account in Settings disagrees with this saved device.")]
    Account,
    #[error("The Azure machine is not running; connecting to it starts it.")]
    NotRunning,
    /// Azure has none of the size to give, at that priority or in that place.
    #[error("Azure has no capacity for this size: {0}")]
    SkuNotAvailable(String),
}
