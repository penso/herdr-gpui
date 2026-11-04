#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Client(#[from] sheprd_core_client::Error),
    #[error("The Herdr 2 terminal connection was lost")]
    Disconnected,
    #[error("This action is unavailable for Herdr 2 terminals")]
    Unsupported,
    // keep-sorted end
}
