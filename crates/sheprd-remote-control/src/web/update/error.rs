//! Why a phone's update request was refused.

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("Sheprd is not open on this computer")]
    AppClosed,
}
