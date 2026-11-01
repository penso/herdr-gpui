//! Why this computer's listening ports could not be read or acted on.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Still acting on the last processes. Try again in a moment.")]
    Busy,
    #[error("Could not list this computer's listening ports.")]
    Scan(#[source] herdr_client::Error),
    #[error("The port list stopped updating. Open the Ports section again to retry.")]
    Stopped,
    #[error("Neither ss nor lsof is installed, so listening ports cannot be read.")]
    Tool,
    #[error("Listing and signalling this computer's ports needs macOS or Linux.")]
    Unsupported,
    #[error("Could not start reading this computer's ports.")]
    Worker(#[source] std::io::Error),
    // keep-sorted end
}
