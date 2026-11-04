//! Why the `[ssh]` table could not be loaded or edited.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("ssh must be a table and ssh.skip_host_key_verification a list of SSH targets")]
    InvalidTable,
    #[error("ssh.skip_host_key_verification: {0:?} is not an SSH target")]
    InvalidTarget(String),
    #[error("ssh.skip_host_key_verification must list at most {0} hosts")]
    TooMany(usize),
    // keep-sorted end
}
