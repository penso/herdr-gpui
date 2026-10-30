#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Core(#[from] herdr_core_client::Error),
    #[error("This build has no herdr-core beside it; install herdr-core from releases.herdr.dev")]
    NotBundled,
    #[error("Could not start a worker for the terminal: {0}")]
    Worker(#[source] std::io::Error),
    // keep-sorted end
}
