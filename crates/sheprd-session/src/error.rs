use crate::{panes, saved};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Panes(#[from] panes::Error),
    #[error(transparent)]
    Saved(#[from] saved::Error),
    // keep-sorted end
}
