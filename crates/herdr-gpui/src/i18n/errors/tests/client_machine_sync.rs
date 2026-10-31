use super::*;
use herdr_client::machine_sync::Error;
use std::{io, path::PathBuf, process::ExitStatus};

/// `ssh-keygen` exiting with status 1, in the system's words.
fn status() -> ExitStatus {
    #[cfg(unix)]
    let status = std::os::unix::process::ExitStatusExt::from_raw(256);
    #[cfg(windows)]
    let status = std::os::windows::process::ExitStatusExt::from_raw(1);
    status
}

#[test]
fn every_machine_sync_variant_has_a_message() {
    assert_localized(&[
        Error::AuthorizedKeys {
            path: PathBuf::from("/home/me/.ssh/authorized_keys"),
            source: io::Error::other("read-only"),
        },
        Error::Catalog(Box::new(herdr_client::Error::CatalogLimit)),
        Error::Changed,
        Error::Home,
        Error::KeyRead(io::Error::other("gone")),
        Error::Keygen(status()),
        Error::KeygenSpawn(io::Error::other("missing")),
        Error::PublicKey,
        Error::Reply,
        Error::Script(Box::new(herdr_client::Error::ScriptTimeout)),
        Error::StateSchema(serde_json::from_str::<u8>("x").unwrap_err()),
        Error::StateValue,
    ]);
}

#[test]
fn a_machine_that_cannot_be_reached_says_why_in_the_scripts_words() {
    let error = Error::Script(Box::new(herdr_client::Error::ScriptTimeout));
    assert_eq!(
        error.localize(),
        herdr_client::Error::ScriptTimeout.localize()
    );
}
