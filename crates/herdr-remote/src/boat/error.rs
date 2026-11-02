//! Boat failures keep their category and cause until the UI displays them.
//! Remote text is bounded diagnostic payload, never a credential.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} must be https (http only on loopback) with no credentials, query, or fragment.")]
    Url(&'static str),
    #[error("{0} is not valid.")]
    Field(crate::cloud::Value),
    #[error("{0} must be UTF-8.")]
    Encoding(crate::cloud::Value),
    #[error("Boat needs an API key. Save one in Settings > Cloud devices.")]
    MissingKey,
    #[error("Boat rejected the API key.")]
    Authentication,
    #[error("Boat denied access. Check the API key's scopes and sandbox access.")]
    Forbidden,
    #[error("{0}")]
    Status(#[from] Status),
    #[error("Boat network request failed or timed out.")]
    Network(#[source] ureq::Error),
    #[error("Boat response could not be read within the size/time limit.")]
    Read(#[source] io::Error),
    #[error("Boat response exceeded the size limit.")]
    Size,
    #[error("Invalid Boat JSON response.")]
    Json(#[source] serde_json::Error),
    #[error("Invalid Boat HTTP header.")]
    Header(#[source] ureq::http::header::InvalidHeaderValue),
    #[error("Boat sandbox failed: {0}")]
    Sandbox(String),
    #[error("The Boat sandbox was deleted.")]
    Deleted,
    #[error("The Boat sandbox did not start within 10 minutes.")]
    StartTimeout,
    #[error("Cancelled.")]
    Cancelled,
    #[error("Could not save or read the pending Boat creation at {path}: {source}")]
    PendingIo {
        path: std::path::PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Another Herdr UI process is creating or recovering a Boat sandbox for this account.")]
    PendingBusy,
    #[error(
        "The pending Boat creation is too old to retry safely. Check the Boat dashboard for its sandbox before forgetting the pending request in Settings > Cloud devices > Boat."
    )]
    PendingExpired,
    #[error(
        "Boat creation may have succeeded. Use Recover creation in Settings > Cloud devices > Boat to retry the saved request: {0}"
    )]
    PendingCreation(#[source] Box<Error>),
    #[error("Could not read the clock for Boat creation recovery: {0}")]
    Clock(#[source] std::time::SystemTimeError),
    #[error("Boat returned SSH connection data this app does not understand.")]
    Gateway,
    #[error("Boat devices need ssh and ssh-keygen on the PATH; one could not be run.")]
    SshMissing(#[source] io::Error),
    #[error("Could not create or read the Boat SSH key.")]
    SshKey(#[source] io::Error),
    #[error("ssh-keygen could not create the Boat SSH key.")]
    SshKeygen,
    #[error("Could not prepare the SSH command for Boat.")]
    GatewayFile(#[source] io::Error),
    #[error(
        "Boat sandbox {id} exists, but its device could not be saved: {source}. Keep this ID to recover the sandbox."
    )]
    SaveDevice {
        id: String,
        #[source]
        source: Box<crate::Error>,
    },
    #[error(
        "Boat sandbox {id} is saved. Use Finish setup in Settings > Cloud devices > Boat to retry: {source}"
    )]
    Setup {
        id: String,
        #[source]
        source: Box<crate::Error>,
    },
    #[error("This device belongs to a Boat account that is no longer configured in [boat].")]
    Account,
    #[error("{0}")]
    Bridge(#[source] herdr_client::Error),
    #[error(transparent)]
    Cloud(#[from] crate::cloud::Error),
    #[error("Could not load the Boat SSH private key: {0}")]
    KeyFile(#[source] Box<crate::Error>),
    #[error("{0}")]
    Storage(#[source] Box<crate::Error>),
}

/// A non-success Boat reply. Boat explains failures in `message`; it is
/// bounded and stripped of control characters before display.
#[derive(Debug, thiserror::Error)]
#[error("Boat request failed (HTTP {code}){}", message.as_deref().map(|m| format!(": {m}")).unwrap_or_default())]
pub struct Status {
    pub code: u16,
    pub message: Option<String>,
}
