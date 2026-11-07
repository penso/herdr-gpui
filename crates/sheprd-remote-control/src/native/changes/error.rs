//! Why a phone's question about a workspace has no answer.

use crate::control::changes::Failure;
use axum::http::StatusCode;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    // keep-sorted start
    #[error("too many questions are waiting for Sheprd")]
    Busy,
    #[error(transparent)]
    Failed(#[from] Failure),
    #[error("Sheprd is not open on this computer")]
    NoDesktop,
    #[error("Sheprd did not answer in time")]
    TimedOut,
    // keep-sorted end
}

impl Error {
    pub(crate) fn status(&self) -> StatusCode {
        match self {
            Self::Busy => StatusCode::TOO_MANY_REQUESTS,
            Self::Failed(Failure::Gone | Failure::NoFile) => StatusCode::NOT_FOUND,
            Self::Failed(Failure::NoBase | Failure::NoLog | Failure::NoRepository) => {
                StatusCode::CONFLICT
            }
            Self::Failed(Failure::Unreadable) => StatusCode::BAD_GATEWAY,
            Self::NoDesktop => StatusCode::SERVICE_UNAVAILABLE,
            Self::TimedOut => StatusCode::GATEWAY_TIMEOUT,
        }
    }
}
