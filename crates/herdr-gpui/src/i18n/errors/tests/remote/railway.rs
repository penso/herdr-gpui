use super::*;
use herdr_remote::railway::Error;
use std::path::PathBuf;

#[test]
fn every_variant_has_a_message() {
    let key = || PathBuf::from("/home/me/.ssh/railway");
    assert_localized(&[
        Error::KeygenMissing(io()),
        Error::Keygen(Some(1)),
        Error::Keygen(None),
        Error::KeygenTimeout,
        Error::UnusableKey(key()),
        Error::KeyName,
        Error::KeyExists(key()),
        Error::KeyPath,
        Error::KeyFile {
            path: key(),
            source: io(),
        },
        Error::Store {
            path: PathBuf::from("/tmp/railway.json"),
            source: io(),
        },
        Error::NoIdentity,
        Error::KeyMissing(key()),
        Error::KeyChanged(key()),
        Error::PublicKeyMismatch(key()),
        Error::Terminal(io()),
        Error::TerminalUnsupported,
        Error::CreateTimeout,
        Error::CreateRefused("key already used".into()),
        Error::Unreachable("connection refused".into()),
        Error::Cloud(herdr_remote::cloud::Error::Cancelled),
    ]);
}

#[test]
fn the_bridge_shows_the_client_error() {
    assert_localized(&[Error::Bridge(client())]);
}
