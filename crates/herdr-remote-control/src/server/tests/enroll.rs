//! `enroll` and `revoke`, which Herdr UI on another computer runs here over
//! SSH: their command lines, and the daemon's answer over its control
//! socket.

use super::*;
use crate::control::{
    DeviceId, Request, Response,
    hosts::{Enroll, HostRevoke},
};

#[test]
fn enroll_and_revoke_take_an_optional_state_directory() {
    assert_eq!(
        parse(args(&["enroll"])).unwrap(),
        Invocation::Enroll { state_dir: None }
    );
    assert_eq!(
        parse(args(&["enroll", "--state-dir", "/tmp/remote"])).unwrap(),
        Invocation::Enroll {
            state_dir: Some(PathBuf::from("/tmp/remote"))
        }
    );
    assert_eq!(
        parse(args(&["revoke", "--device", "d1"])).unwrap(),
        Invocation::Revoke {
            state_dir: None,
            device: "d1".into()
        }
    );
    assert_eq!(
        parse(args(&["revoke"])).unwrap_err(),
        ParseError::Missing("--device")
    );
    assert_eq!(
        parse(args(&["revoke", "--device", ""])).unwrap_err(),
        ParseError::Value { option: "--device" }
    );
    // `--device` belongs to revoke alone.
    assert_eq!(
        parse(args(&["enroll", "--device", "d1"])).unwrap_err(),
        ParseError::Unknown("--device".into())
    );
}

#[test]
fn the_daemon_enrolls_a_phone_and_says_where_it_answers() {
    let dir = tempfile::tempdir().unwrap();
    let state = web::tests::state(dir.path());
    let (stop, _) = watch::channel(false);
    let key = p256::ecdsa::SigningKey::from_slice(&[7; 32]).unwrap();
    let public_key = crate::auth::encode(key.verifying_key().to_sec1_point(false).as_bytes());
    let response = control::answer(
        &state,
        &stop,
        Request::Enroll(Enroll {
            name: "iPhone".into(),
            public_key,
            via: "MacBook Pro".into(),
            replace: None,
        }),
    );
    let Response::Enrolled(enrolled) = response else {
        panic!("not enrolled: {response:?}");
    };
    assert_eq!(enrolled.computer, state.computer);
    assert_eq!(enrolled.fingerprint, state.fingerprint);
    assert_eq!(enrolled.origins, state.origins());
    let Response::Devices { devices } = control::answer(&state, &stop, Request::Devices) else {
        panic!("no devices");
    };
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id, enrolled.device);
    assert_eq!(devices[0].via.as_deref(), Some("MacBook Pro"));
    // A bad key is refused.
    assert!(matches!(
        control::answer(
            &state,
            &stop,
            Request::Enroll(Enroll {
                name: "iPhone".into(),
                public_key: "nope".into(),
                via: "MacBook Pro".into(),
                replace: None,
            })
        ),
        Response::Error { .. }
    ));
}

#[test]
fn host_revocations_are_listed_and_cleared() {
    let dir = tempfile::tempdir().unwrap();
    let state = web::tests::state(dir.path());
    let (stop, _) = watch::channel(false);
    let Response::HostRevokes { revokes } = control::answer(&state, &stop, Request::HostRevokes)
    else {
        panic!("no revokes");
    };
    assert!(revokes.is_empty());
    // Clearing one never owed is fine.
    assert_eq!(
        control::answer(
            &state,
            &stop,
            Request::HostRevoked(HostRevoke {
                host: crate::control::hosts::HostId("ssh:devbox".into()),
                device: DeviceId("d1".into()),
            })
        ),
        Response::Done
    );
}
