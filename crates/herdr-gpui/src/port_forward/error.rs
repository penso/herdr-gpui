//! Why a port could not be forwarded from a remote host.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Port {0} is already forwarded from this host.")]
    Duplicate(u16),
    #[error("At most {0} ports can be forwarded at once.")]
    Limit(usize),
    #[error("Enter a port number from 1 to 65535.")]
    Port,
    // keep-sorted end
}
