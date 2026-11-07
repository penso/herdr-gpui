pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Why a cloud machine, a host script, or a teleport could not finish. Each
/// part keeps its own error; this gathers them for callers that run several.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("{0}")]
    Client(#[from] sheprd_client::Error),
    #[cfg(feature = "cloud")]
    #[error(transparent)]
    Cloud(#[from] crate::cloud::Error),
    /// The app's config could not be loaded; its own error is the source.
    #[error(transparent)]
    Config(crate::config::LoadError),
    #[error(transparent)]
    Credentials(#[from] sheprd_credentials::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("Sheprd's config is not available to cloud providers.")]
    NoConfig,
    #[error(transparent)]
    Path(#[from] sheprd_client::local_path::PathError),
    #[error("preferences path has no parent")]
    PreferencesPath,
    #[cfg(feature = "cloud")]
    #[error(transparent)]
    Provider(#[from] crate::cloud::ProviderError),
    #[cfg(feature = "cloud")]
    #[error(transparent)]
    SandboxImage(#[from] crate::cloud::image::Error),
    #[error(transparent)]
    Teleport(#[from] crate::teleport::Error),
    // keep-sorted end
}
