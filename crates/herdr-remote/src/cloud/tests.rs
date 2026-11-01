#![allow(clippy::unwrap_used)]
use super::*;
use herdr_client::ConnectTarget;
use std::sync::atomic::AtomicBool;

// A loopback API for the providers that have one; Railway is reached over SSH.
mod discard;
#[cfg(any(feature = "boat", feature = "coder", feature = "daytona"))]
pub mod server;

#[test]
fn every_provider_names_itself_and_its_machines() {
    for &provider in CloudProvider::ALL {
        assert!(!name(provider).is_empty());
        assert!(!noun(provider).is_empty());
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

#[test]
fn cloud_devices_are_offered_only_where_they_can_connect() {
    assert_eq!(unavailable().is_some(), cfg!(windows));
}

#[test]
fn deleting_at_a_provider_names_the_machine_and_what_goes_with_it() {
    for &provider in CloudProvider::ALL {
        let Some(deletion) = deletion(provider) else {
            continue;
        };
        assert!(!(deletion.noun)("machine-1").is_empty());
        // Read between "at <provider>" and " and remove it".
        assert!(
            deletion.leftovers.is_empty()
                || (deletion.leftovers.starts_with(", ") && deletion.leftovers.ends_with(',')),
            "{:?}",
            deletion.leftovers
        );
    }
    #[cfg(feature = "azure")]
    assert!(deletion(CloudProvider::Azure).is_some());
}
