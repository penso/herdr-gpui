//! Railway failures keep their category and cause until the UI displays them.
//! Remote text is bounded diagnostic payload, never a credential.

use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Railway devices need ssh-keygen on the PATH; it could not be run.")]
    KeygenMissing(#[source] io::Error),
    #[error("ssh-keygen could not create or read the key (exit status {0:?}).")]
    Keygen(Option<i32>),
    #[error("ssh-keygen did not finish within 30 seconds.")]
    KeygenTimeout,
    #[error(
        "{} is not a usable SSH private key. A passphrase-protected key needs its .pub file next to it and must be loaded in your SSH agent.",
        .0.display()
    )]
    UnusableKey(PathBuf),
    #[error("{}", super::keys::NAME_RULE)]
    KeyName,
    #[error("{} already exists, and Sheprd never replaces a key. Choose another name.", .0.display())]
    KeyExists(PathBuf),
    #[error("Choose an absolute path to an SSH private key.")]
    KeyPath,
    #[error("Could not use the key file {}.", path.display())]
    KeyFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Could not update {}.", path.display())]
    Store {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("This device has no saved key path. Remove it and set it up again.")]
    NoIdentity,
    #[error(
        "The SSH key for this device is missing: {}. It is never replaced with a new one; restore it, or remove the device.",
        .0.display()
    )]
    KeyMissing(PathBuf),
    #[error(
        "The key at {} is not the one this device was set up with, so it was not used. Restore that key, or remove the device and set it up again.",
        .0.display()
    )]
    KeyChanged(PathBuf),
    #[error(
        "The public key beside {} does not match its private key. Restore the matching .pub file before connecting.",
        .0.display()
    )]
    PublicKeyMismatch(PathBuf),
    #[error("Could not open a terminal to create the Railway VM.")]
    Terminal(#[source] io::Error),
    #[error("Creating the VM needs a terminal, which this build of Sheprd cannot open.")]
    TerminalUnsupported,
    #[error("Railway did not finish creating the VM within 90 seconds.")]
    CreateTimeout,
    #[error("Railway did not create a VM for this key: {0}")]
    CreateRefused(String),
    #[error("Could not reach railway.new: {0}")]
    Unreachable(String),
    #[error("{0}")]
    Bridge(#[source] sheprd_client::Error),
    #[error(transparent)]
    Cloud(#[from] crate::cloud::Error),
}
