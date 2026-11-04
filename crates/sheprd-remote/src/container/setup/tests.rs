#![allow(clippy::unwrap_used)]
use super::*;
use std::sync::TryLockError;

// One test, as the places are counted process-wide.
#[test]
fn a_place_is_counted_with_the_saved_devices_under_one_lock() {
    let first = Slot::take(|| Ok(6), 8).unwrap();
    // A creation finishing now cannot save between the count and the take.
    let second = Slot::take(
        || {
            assert!(matches!(CREATING.try_lock(), Err(TryLockError::WouldBlock)));
            Ok(6)
        },
        8,
    )
    .unwrap();
    assert!(matches!(
        Slot::take(|| Ok(6), 8),
        Err(crate::Error::Provider(cloud::ProviderError::Container(
            super::super::Error::Limit(8)
        )))
    ));
    drop(first);
    // The first saved its device as it gave its place back.
    assert!(matches!(
        Slot::take(|| Ok(7), 8),
        Err(crate::Error::Provider(cloud::ProviderError::Container(
            super::super::Error::Limit(8)
        )))
    ));
    drop(second);
    drop(Slot::take(|| Ok(7), 8).unwrap());
    assert!(matches!(
        Slot::take(|| Err(super::super::Error::Cancelled.into()), 8),
        Err(crate::Error::Provider(cloud::ProviderError::Container(
            super::super::Error::Cancelled
        )))
    ));
    assert_eq!(*CREATING.lock().unwrap(), 0);
}
