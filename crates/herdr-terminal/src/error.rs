pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The crate's failures. Emulation itself cannot fail: any bytes are valid
/// input, and sizes arrive already checked as [`crate::Size`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Backend(#[from] herdr_backend::Error),
    // keep-sorted end
}
