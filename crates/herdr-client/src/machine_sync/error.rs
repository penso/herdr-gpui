//! Why syncing the machine list, or one machine of it, failed.
use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not update this computer's authorized SSH keys (~/.ssh/authorized_keys)")]
    AuthorizedKeys {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("The machine's saved devices could not be read")]
    Catalog(#[source] Box<crate::Error>),
    #[error("The machine's saved devices changed during the sync; it is synced next time")]
    Changed,
    #[error("This computer has no home folder for its SSH key")]
    Home,
    #[error("ssh-keygen could not create the herdr SSH key ({0})")]
    Keygen(std::process::ExitStatus),
    #[error("ssh-keygen could not be run; install OpenSSH")]
    KeygenSpawn(#[source] io::Error),
    #[error("The herdr SSH key (~/.ssh/herdr_ed25519) could not be read")]
    KeyRead(#[source] io::Error),
    #[error("The machine has a herdr SSH key that is not an Ed25519 key")]
    PublicKey,
    #[error("The machine answered in a form machine sync does not understand")]
    Reply,
    /// The machine could not be reached, or its script failed there.
    #[error(transparent)]
    Script(Box<crate::Error>),
    #[error("The machine sync settings could not be read")]
    StateSchema(#[source] serde_json::Error),
    #[error("The machine sync settings are not valid; check this computer's address and name")]
    StateValue,
    // keep-sorted end
}
