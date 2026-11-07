//! Why a dispatch could not start, or its saved history could not be read.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Another host is still being set up. Wait for it to finish.")]
    Busy,
    #[error("{0} cannot be reached by script. Choose another host.")]
    HostUnavailable(String),
    #[error("Invalid saved dispatch history")]
    InvalidHistory,
    // keep-sorted end
}
