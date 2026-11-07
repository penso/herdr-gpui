pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Another subscriber was installed first; it is never replaced.
    #[error(transparent)]
    Subscriber(#[from] tracing::subscriber::SetGlobalDefaultError),
}
