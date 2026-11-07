use super::{MAX_SAVED_BYTES, MAX_SAVED_TABS};

/// Why the app's saved tabs could not be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("The saved tabs are not readable")]
    Format(#[from] serde_json::Error),
    #[error("The saved tabs are larger than {MAX_SAVED_BYTES} bytes")]
    Large,
    #[error("There are more than {MAX_SAVED_TABS} saved tabs")]
    Many,
    // keep-sorted end
}
