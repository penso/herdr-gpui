//! Why a device's tags could not be read, offered, or saved.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("The host's tags are not ones this app can read")]
    Answer(#[source] serde_json::Error),
    #[error(transparent)]
    Daemon(#[from] sheprd_remote_control::app::Error),
    #[error("Could not run sheprd-remote-control on the host")]
    Run(#[source] sheprd_client::Error),
    #[error("Could not save device tags to {}", path.display())]
    Save {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Tags cannot be shared with this kind of device")]
    Unsupported,
    // keep-sorted end
}
