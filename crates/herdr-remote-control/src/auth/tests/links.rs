//! Pairing a phone with the hosts Herdr UI connects to: what the desktop's
//! Allow covers, the requests Herdr UI carries out, what each host
//! answered, and the revocations owed to hosts.

use super::*;
use crate::auth::links::{QUEUED_TTL_MS, TAKEN_TTL_MS};
use crate::control::hosts::{Enroll, Enrolled, Host, HostId, HostOutcome, HostRevoke, HostState};

fn host(id: &str, name: &str) -> Host {
    Host {
        id: HostId(id.into()),
        name: name.into(),
    }
}

pub(super) fn devbox() -> HostId {
    HostId("ssh:devbox".into())
}

fn gpu() -> HostId {
    HostId("ssh:gpu".into())
}

pub(super) fn enrolled(device: &str) -> Enrolled {
    Enrolled {
        device: DeviceId(device.into()),
        computer: "devbox".into(),
        fingerprint: "11:22:33".into(),
        origins: vec!["https://100.64.0.7:7443".into()],
    }
}

/// An auth that knows two hosts and holds a phone allowed on `allowed`.
pub(super) fn paired(dir: &Path, allowed: &[HostId]) -> (Auth, DeviceId) {
    let mut auth = auth(dir);
    auth.set_hosts(vec![
        host("ssh:devbox", "devbox"),
        host("ssh:gpu", "gpu box"),
    ])
    .unwrap();
    let offer = auth.mint(T0, &origins(), FINGERPRINT).unwrap();
    let claim = auth
        .claim(
            IpAddr::from([192, 168, 1, 30]),
            &token(&offer),
            "iPhone",
            &public_key(&signing_key(1)),
            T0,
        )
        .unwrap();
    let device = match auth
        .answer_with_hosts(&offer.id, true, &claim.code, allowed, T0)
        .unwrap()
    {
        PairingState::Approved { device } => device,
        state => panic!("not approved: {state:?}"),
    };
    (auth, device)
}

pub(super) fn state_of(auth: &mut Auth, device: &DeviceId, host: &HostId, now: u64) -> HostState {
    auth.host_links(device, now)
        .unwrap()
        .into_iter()
        .find(|link| &link.host == host)
        .unwrap()
        .state
}

#[test]
fn allow_covers_the_ticked_hosts_and_offers_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[devbox(), HostId("ssh:unknown".into())]);
    let links = auth.host_links(&device, T0).unwrap();
    assert_eq!(
        links
            .iter()
            .map(|link| (link.host.0.as_str(), link.name.as_str(), &link.state))
            .collect::<Vec<_>>(),
        vec![
            ("ssh:devbox", "devbox", &HostState::Allowed),
            ("ssh:gpu", "gpu box", &HostState::Offered),
        ]
    );
    // What the desktop allowed outlives a restart.
    let mut reloaded = super::auth(dir.path());
    reloaded
        .set_hosts(vec![host("ssh:devbox", "devbox")])
        .unwrap();
    assert_eq!(
        state_of(&mut reloaded, &device, &devbox(), T0),
        HostState::Allowed
    );
}

#[test]
fn an_allowed_host_is_paired_through_herdr_ui() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[devbox()]);
    let key = public_key(&signing_key(2));
    assert_eq!(
        auth.ask_host(&device, &devbox(), &key, T0).unwrap(),
        HostState::Pending
    );
    let taken = auth.take_host_pairings(T0);
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].device, device);
    assert_eq!(taken[0].host, devbox());
    assert_eq!(taken[0].name, "iPhone");
    assert_eq!(taken[0].public_key, key);
    assert!(!taken[0].confirm);
    assert_eq!(taken[0].replace, None);
    // Taken once.
    assert!(auth.take_host_pairings(T0).is_empty());
    // While Herdr UI works on it, the phone cannot start another.
    assert!(matches!(
        auth.ask_host(&device, &devbox(), &key, T0),
        Err(Error::Links(crate::auth::links::Error::Busy))
    ));

    auth.host_paired(
        &device,
        &devbox(),
        None,
        HostOutcome::Paired(enrolled("d-host")),
        T0,
    )
    .unwrap();
    assert_eq!(
        state_of(&mut auth, &device, &devbox(), T0),
        HostState::Paired(enrolled("d-host"))
    );
    // The host's answer is kept with the device.
    let mut reloaded = super::auth(dir.path());
    assert_eq!(
        state_of(&mut reloaded, &device, &devbox(), T0),
        HostState::Paired(enrolled("d-host"))
    );
}

#[test]
fn pairing_a_paired_host_again_replaces_its_device() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[devbox()]);
    let key = public_key(&signing_key(2));
    auth.ask_host(&device, &devbox(), &key, T0).unwrap();
    auth.take_host_pairings(T0);
    auth.host_paired(
        &device,
        &devbox(),
        None,
        HostOutcome::Paired(enrolled("d-old")),
        T0,
    )
    .unwrap();
    auth.ask_host(&device, &devbox(), &key, T0 + 1).unwrap();
    let taken = auth.take_host_pairings(T0 + 1);
    assert_eq!(taken[0].replace, Some(DeviceId("d-old".into())));
}

#[test]
fn a_host_not_allowed_asks_the_desktop_first() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[]);
    let key = public_key(&signing_key(2));
    assert_eq!(
        auth.ask_host(&device, &gpu(), &key, T0).unwrap(),
        HostState::Asking
    );
    let taken = auth.take_host_pairings(T0);
    assert!(taken[0].confirm);
    // The user declines: the host is offered again.
    auth.host_paired(&device, &gpu(), None, HostOutcome::Denied, T0)
        .unwrap();
    assert_eq!(state_of(&mut auth, &device, &gpu(), T0), HostState::Offered);
}

#[test]
fn a_failure_is_shown_until_the_phone_retries() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[devbox()]);
    let key = public_key(&signing_key(2));
    auth.ask_host(&device, &devbox(), &key, T0).unwrap();
    auth.take_host_pairings(T0);
    auth.host_paired(
        &device,
        &devbox(),
        None,
        HostOutcome::Failed {
            reason: "x".repeat(2000),
        },
        T0,
    )
    .unwrap();
    let HostState::Failed { reason } = state_of(&mut auth, &device, &devbox(), T0) else {
        panic!("not failed");
    };
    assert_eq!(reason.len(), control::hosts::MAX_REASON);
    assert_eq!(
        auth.ask_host(&device, &devbox(), &key, T0 + 1).unwrap(),
        HostState::Pending
    );
}

#[test]
fn work_herdr_ui_never_finishes_fails_and_untaken_work_lapses() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[devbox(), gpu()]);
    let key = public_key(&signing_key(2));
    auth.ask_host(&device, &devbox(), &key, T0).unwrap();
    auth.take_host_pairings(T0);
    auth.ask_host(&device, &gpu(), &key, T0).unwrap();
    let later = T0 + TAKEN_TTL_MS;
    assert!(matches!(
        state_of(&mut auth, &device, &devbox(), later),
        HostState::Failed { .. }
    ));
    // A late answer is refused.
    assert!(matches!(
        auth.host_paired(
            &device,
            &devbox(),
            None,
            HostOutcome::Paired(enrolled("d")),
            later
        ),
        Err(Error::Links(crate::auth::links::Error::NotPairing))
    ));
    assert_eq!(
        state_of(&mut auth, &device, &gpu(), T0 + QUEUED_TTL_MS),
        HostState::Allowed
    );
}

#[test]
fn unknown_hosts_and_bad_keys_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[devbox()]);
    assert!(matches!(
        auth.ask_host(
            &device,
            &HostId("ssh:elsewhere".into()),
            &public_key(&signing_key(2)),
            T0
        ),
        Err(Error::Links(crate::auth::links::Error::UnknownHost))
    ));
    assert!(matches!(
        auth.ask_host(&device, &devbox(), "not a key", T0),
        Err(Error::InvalidKey)
    ));
    assert!(matches!(
        auth.ask_host(
            &DeviceId("nobody".into()),
            &devbox(),
            &public_key(&signing_key(2)),
            T0
        ),
        Err(Error::UnknownDevice)
    ));
    assert!(matches!(
        auth.set_hosts(vec![host("", "empty")]),
        Err(Error::Links(crate::auth::links::Error::InvalidHost))
    ));
}

#[test]
fn an_answer_with_a_bad_enrollment_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[devbox()]);
    auth.ask_host(&device, &devbox(), &public_key(&signing_key(2)), T0)
        .unwrap();
    auth.take_host_pairings(T0);
    let mut bad = enrolled("d");
    bad.origins = vec!["https://x".into(); control::hosts::MAX_ORIGINS + 1];
    assert!(matches!(
        auth.host_paired(&device, &devbox(), None, HostOutcome::Paired(bad), T0),
        Err(Error::Links(crate::auth::links::Error::InvalidOutcome))
    ));
}

#[test]
fn revoking_a_phone_owes_its_hosts_a_revocation() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[devbox(), gpu()]);
    auth.ask_host(&device, &devbox(), &public_key(&signing_key(2)), T0)
        .unwrap();
    auth.take_host_pairings(T0);
    auth.host_paired(
        &device,
        &devbox(),
        None,
        HostOutcome::Paired(enrolled("d-host")),
        T0,
    )
    .unwrap();
    // `gpu` was allowed but never paired: nothing is owed there.
    assert!(auth.revoke(&device).unwrap());
    let owed = HostRevoke {
        host: devbox(),
        device: DeviceId("d-host".into()),
    };
    assert_eq!(auth.host_revokes(), vec![owed.clone()]);
    // Kept across a restart until Herdr UI says the host let it go.
    let mut reloaded = super::auth(dir.path());
    assert_eq!(reloaded.host_revokes(), vec![owed.clone()]);
    reloaded.host_revoked(&owed).unwrap();
    assert!(reloaded.host_revokes().is_empty());
    assert!(super::auth(dir.path()).host_revokes().is_empty());
}

#[test]
fn an_enrolled_phone_signs_in_with_its_own_key() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(3);
    let device = auth
        .enroll(
            &Enroll {
                name: "iPhone".into(),
                public_key: public_key(&key),
                via: "MacBook Pro".into(),
                replace: None,
            },
            T0,
        )
        .unwrap();
    assert_eq!(
        auth.devices().get(&device).unwrap().via.as_deref(),
        Some("MacBook Pro")
    );
    let client = IpAddr::from([100, 64, 0, 9]);
    let challenge = auth.challenge(client, &device, T0).unwrap();
    let session = auth
        .verify(&challenge.id, &sign(&key, &challenge.nonce), T0)
        .unwrap();
    assert_eq!(session.device, device);
}

#[test]
fn enrolling_again_replaces_only_an_enrolled_device() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let direct = pair(&mut auth, &signing_key(1), T0);
    let enroll = |replace: Option<DeviceId>| Enroll {
        name: "iPhone".into(),
        public_key: public_key(&signing_key(4)),
        via: "MacBook Pro".into(),
        replace,
    };
    let first = auth.enroll(&enroll(None), T0).unwrap();
    let second = auth.enroll(&enroll(Some(first.clone())), T0).unwrap();
    assert!(auth.devices().get(&first).is_none());
    // A phone paired here directly is not another computer's to replace.
    auth.enroll(&enroll(Some(direct.clone())), T0).unwrap();
    assert!(auth.devices().get(&direct).is_some());
    assert!(auth.devices().get(&second).is_some());
}

#[test]
fn a_device_list_from_before_hosts_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("devices.json"),
        format!(
            r#"{{"version":1,"devices":[{{"id":"d1","name":"iPhone","public_key":"{}","created_ms":1}}]}}"#,
            public_key(&signing_key(1))
        ),
    )
    .unwrap();
    let devices = Devices::load(dir.path()).unwrap();
    let record = devices.get(&DeviceId("d1".into())).unwrap();
    assert_eq!(record.via, None);
    assert!(record.links.is_empty());
}

#[test]
fn a_host_that_answers_after_the_phone_was_revoked_is_owed_a_revocation() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[devbox()]);
    auth.ask_host(&device, &devbox(), &public_key(&signing_key(2)), T0)
        .unwrap();
    auth.take_host_pairings(T0);
    // Revoked while Herdr UI was still enrolling it on the host.
    assert!(auth.revoke(&device).unwrap());
    assert!(matches!(
        auth.host_paired(
            &device,
            &devbox(),
            None,
            HostOutcome::Paired(enrolled("d-late")),
            T0
        ),
        Err(Error::Links(crate::auth::links::Error::NotPairing))
    ));
    let owed = HostRevoke {
        host: devbox(),
        device: DeviceId("d-late".into()),
    };
    assert_eq!(auth.host_revokes(), vec![owed.clone()]);
    assert_eq!(super::auth(dir.path()).host_revokes(), vec![owed]);
}

#[test]
fn a_host_that_answers_after_its_request_timed_out_is_owed_a_revocation() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, device) = paired(dir.path(), &[devbox()]);
    auth.ask_host(&device, &devbox(), &public_key(&signing_key(2)), T0)
        .unwrap();
    auth.take_host_pairings(T0);
    let late = T0 + TAKEN_TTL_MS;
    assert!(
        auth.host_paired(
            &device,
            &devbox(),
            None,
            HostOutcome::Paired(enrolled("d-late")),
            late
        )
        .is_err()
    );
    assert_eq!(auth.host_revokes().len(), 1);
    // A failure or a refusal that comes late owes nothing.
    assert!(
        auth.host_paired(&device, &devbox(), None, HostOutcome::Denied, late)
            .is_err()
    );
    assert_eq!(auth.host_revokes().len(), 1);
}
