//! A host attempt that outlasts the phone's retry, and a revocation whose
//! save failed, are both retried without losing what they owe.

use super::links::{devbox, enrolled, paired, state_of};
use super::*;
use crate::control::hosts::{HostOutcome, HostRevoke, HostState};
use std::fs;

#[test]
fn an_attempt_that_outlasted_a_retry_cannot_answer_it() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[devbox()]);
    let first = public_key(&signing_key(2));
    auth.ask_host(&device, &devbox(), &first, T0).unwrap();
    auth.take_host_pairings(T0);
    // The phone gave up and asked again with a new key, which Herdr UI
    // took while the first attempt still ran.
    let late = T0 + crate::auth::links::TAKEN_TTL_MS;
    let second = public_key(&signing_key(3));
    auth.ask_host(&device, &devbox(), &second, late).unwrap();
    assert_eq!(auth.take_host_pairings(late).len(), 1);
    assert!(
        auth.host_paired(
            &device,
            &devbox(),
            Some(&first),
            HostOutcome::Paired(enrolled("d-old")),
            late
        )
        .is_err()
    );
    assert_eq!(
        state_of(&mut auth, &device, &devbox(), late),
        HostState::Pending
    );
    // The host holds a device for the old key that nobody will use.
    assert_eq!(
        auth.host_revokes(),
        vec![HostRevoke {
            host: devbox(),
            device: DeviceId("d-old".into()),
        }]
    );
    auth.host_paired(
        &device,
        &devbox(),
        Some(&second),
        HostOutcome::Paired(enrolled("d-new")),
        late,
    )
    .unwrap();
    assert!(matches!(
        state_of(&mut auth, &device, &devbox(), late),
        HostState::Paired(enrolled) if enrolled.device.0 == "d-new"
    ));
}

#[test]
fn a_revocation_whose_save_failed_is_saved_by_the_retry() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[devbox()]);
    let key = public_key(&signing_key(2));
    auth.ask_host(&device, &devbox(), &key, T0).unwrap();
    auth.take_host_pairings(T0);
    auth.host_paired(
        &device,
        &devbox(),
        Some(&key),
        HostOutcome::Paired(enrolled("d-host")),
        T0,
    )
    .unwrap();
    // A directory where the file goes makes its write fail.
    let blocker = dir.path().join("host-revokes.json");
    fs::create_dir(&blocker).unwrap();
    fs::write(blocker.join("keep"), b"").unwrap();
    assert!(auth.revoke(&device).is_err());
    assert!(auth.host_revokes().is_empty());
    fs::remove_dir_all(&blocker).unwrap();
    assert!(auth.revoke(&device).unwrap());
    let owed = HostRevoke {
        host: devbox(),
        device: DeviceId("d-host".into()),
    };
    assert_eq!(super::auth(dir.path()).host_revokes(), vec![owed]);
}

#[test]
fn a_failed_save_tells_herdr_ui_to_try_again() {
    let error = Error::Store {
        operation: StoreOperation::Replace,
        path: "devices.json".into(),
        source: std::io::ErrorKind::PermissionDenied.into(),
    };
    assert_eq!(
        control::ErrorCode::from(&error),
        control::ErrorCode::Unsaved
    );
    assert_eq!(
        control::ErrorCode::from(&Error::Links(crate::auth::links::Error::NotPairing)),
        control::ErrorCode::Other
    );
}
