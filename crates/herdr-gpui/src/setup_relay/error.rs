//! Why a device setup's terminal relay failed.

use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Herdr closed the setup before it finished. Start it again from Add Device.")]
    Closed,
    #[error("This setup is no longer waiting. Start it again from Add Device.")]
    Connect(#[source] io::Error),
    #[error("{} is not a private folder of this user, so the setup cannot listen there.", .0.display())]
    Dir(PathBuf),
    #[error("The setup terminal sent something unreadable.")]
    Frame,
    #[error("The device setup needs a terminal.")]
    Terminal,
    // keep-sorted end
}
