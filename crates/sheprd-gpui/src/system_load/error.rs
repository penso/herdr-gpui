//! Why reading a host's CPU and memory failed.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("CPU and memory output was not understood.")]
    Output,
    #[error("Could not read CPU and memory on this host.")]
    Remote(#[source] Box<crate::Error>),
    /// The host's `uname -s`, bounded, so the message names what it is.
    #[error("CPU and memory cannot be read on {0:?} hosts.")]
    Unsupported(String),
    // keep-sorted end
}
