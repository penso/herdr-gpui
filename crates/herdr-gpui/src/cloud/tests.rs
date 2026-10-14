#![allow(clippy::unwrap_used)]
use super::*;

// A loopback API for the providers that have one; Railway is reached over SSH.
#[cfg(any(feature = "coder", feature = "daytona"))]
pub(crate) mod server;

#[test]
fn every_provider_names_itself_and_its_machines() {
    for &provider in CloudProvider::ALL {
        assert!(!provider.key().contains(':'), "keys prefix endpoint IDs");
    }
}

#[test]
fn a_non_cloud_target_is_refused() {
    let Err(error) = connect(&ConnectTarget::Local, &AtomicBool::new(true)) else {
        panic!("a local target is not a cloud machine");
    };
    assert!(error.get_ref().is_some_and(|e| e.is::<Error>()));
}
