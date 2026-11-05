pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Why a continued session's earlier messages could not be shown.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("more than one log holds this session")]
    Ambiguous,
    #[error(transparent)]
    Mirror(#[from] crate::mirror::Error),
    #[error("neither the agent's config directory nor HOME is set")]
    NoHome,
    #[error("no log holds this session")]
    NotFound,
    #[error(transparent)]
    Observe(#[from] crate::observe::Error),
}
