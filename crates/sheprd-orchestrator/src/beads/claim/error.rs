//! Why a bead could not be claimed for an agent.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Someone else holds it, named when `bd` said who.
    #[error("{id} is already claimed by {holder}")]
    Held { id: String, holder: String },
    #[error("{id} is already claimed")]
    HeldByUnknown { id: String },
}
