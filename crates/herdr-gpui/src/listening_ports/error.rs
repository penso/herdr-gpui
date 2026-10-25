//! Why reading a host's listening ports failed.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not read listening ports on this host.")]
    Scan(#[source] Box<crate::Error>),
    #[error("Neither ss nor lsof is installed on this host, so listening ports cannot be read.")]
    Tool,
    #[error("Listening ports cannot be read on this platform.")]
    Unsupported,
    // keep-sorted end
}
