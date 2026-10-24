use super::*;
use crate::auth::session::{CHALLENGE_TTL_MS, MAX_CHALLENGES_PER_DEVICE, SESSION_TTL_MS};

/// A store holding device 1, with its key.
fn store(dir: &Path) -> (Devices, DeviceId, SigningKey) {
    let mut devices = Devices::load(dir).unwrap();
    let device = new_device(1, "iPhone");
    let id = device.id.clone();
    devices.add(device, T0).unwrap();
    (devices, id, signing_key(1))
}

#[test]
fn a_signed_challenge_starts_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let (devices, id, key) = store(dir.path());
    let mut sessions = Sessions::default();
    let challenge = sessions.challenge(&id, T0, &devices).unwrap();
    assert_eq!(challenge.expires_ms, T0 + CHALLENGE_TTL_MS);
    assert_eq!(decode_fixed::<32>(&challenge.nonce).unwrap().len(), 32);
    let session = sessions
        .verify(&challenge.id, &sign(&key, &challenge.nonce), T0, &devices)
        .unwrap();
    assert_eq!(session.device, id);
    assert_eq!(session.expires_ms, T0 + SESSION_TTL_MS);
    assert_eq!(sessions.authenticate(&session.token, T0), Some(id));
}

#[test]
fn a_signature_over_another_nonce_fails() {
    let dir = tempfile::tempdir().unwrap();
    let (devices, id, key) = store(dir.path());
    let mut sessions = Sessions::default();
    let challenge = sessions.challenge(&id, T0, &devices).unwrap();
    let other = sessions.challenge(&id, T0, &devices).unwrap();
    let error = sessions
        .verify(&challenge.id, &sign(&key, &other.nonce), T0, &devices)
        .unwrap_err();
    assert!(matches!(error, Error::InvalidSignature));
}

#[test]
fn a_signature_by_another_key_fails() {
    let dir = tempfile::tempdir().unwrap();
    let (devices, id, _) = store(dir.path());
    let mut sessions = Sessions::default();
    let challenge = sessions.challenge(&id, T0, &devices).unwrap();
    let error = sessions
        .verify(
            &challenge.id,
            &sign(&signing_key(2), &challenge.nonce),
            T0,
            &devices,
        )
        .unwrap_err();
    assert!(matches!(error, Error::InvalidSignature));
}

#[test]
fn a_challenge_answers_once() {
    let dir = tempfile::tempdir().unwrap();
    let (devices, id, key) = store(dir.path());
    let mut sessions = Sessions::default();
    let challenge = sessions.challenge(&id, T0, &devices).unwrap();
    let signature = sign(&key, &challenge.nonce);
    sessions
        .verify(&challenge.id, &signature, T0, &devices)
        .unwrap();
    let error = sessions
        .verify(&challenge.id, &signature, T0, &devices)
        .unwrap_err();
    assert!(matches!(error, Error::UnknownChallenge));
}

#[test]
fn a_failed_answer_uses_the_challenge_up() {
    let dir = tempfile::tempdir().unwrap();
    let (devices, id, key) = store(dir.path());
    let mut sessions = Sessions::default();
    let challenge = sessions.challenge(&id, T0, &devices).unwrap();
    sessions
        .verify(&challenge.id, &encode(&[1; 64]), T0, &devices)
        .unwrap_err();
    let error = sessions
        .verify(&challenge.id, &sign(&key, &challenge.nonce), T0, &devices)
        .unwrap_err();
    assert!(matches!(error, Error::UnknownChallenge));
}

#[test]
fn a_challenge_expires_after_sixty_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let (devices, id, key) = store(dir.path());
    let mut sessions = Sessions::default();
    let late = sessions.challenge(&id, T0, &devices).unwrap();
    let error = sessions
        .verify(
            &late.id,
            &sign(&key, &late.nonce),
            T0 + CHALLENGE_TTL_MS,
            &devices,
        )
        .unwrap_err();
    assert!(matches!(error, Error::UnknownChallenge));

    let timely = sessions.challenge(&id, T0, &devices).unwrap();
    sessions
        .verify(
            &timely.id,
            &sign(&key, &timely.nonce),
            T0 + CHALLENGE_TTL_MS - 1,
            &devices,
        )
        .unwrap();
}

#[test]
fn a_device_holds_a_bounded_number_of_challenges() {
    let dir = tempfile::tempdir().unwrap();
    let (devices, id, key) = store(dir.path());
    let mut sessions = Sessions::default();
    let challenges: Vec<Challenge> = (0..=MAX_CHALLENGES_PER_DEVICE as u64)
        .map(|offset| sessions.challenge(&id, T0 + offset, &devices).unwrap())
        .collect();
    let oldest = &challenges[0];
    let error = sessions
        .verify(&oldest.id, &sign(&key, &oldest.nonce), T0 + 20, &devices)
        .unwrap_err();
    assert!(matches!(error, Error::UnknownChallenge));
    for challenge in &challenges[1..] {
        sessions
            .verify(
                &challenge.id,
                &sign(&key, &challenge.nonce),
                T0 + 20,
                &devices,
            )
            .unwrap();
    }
}

#[test]
fn only_paired_devices_get_challenges() {
    let dir = tempfile::tempdir().unwrap();
    let (devices, _, _) = store(dir.path());
    let error = Sessions::default()
        .challenge(&DeviceId("nope".into()), T0, &devices)
        .unwrap_err();
    assert!(matches!(error, Error::UnknownDevice));
    assert_eq!(
        control::ErrorCode::from(&error),
        control::ErrorCode::UnknownDevice
    );
}

#[test]
fn malformed_signatures_are_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let (devices, id, key) = store(dir.path());
    let mut sessions = Sessions::default();
    let good = sign(&key, &sessions.challenge(&id, T0, &devices).unwrap().nonce);
    for bad in [
        String::new(),
        "!".repeat(86),
        encode(&[0; 64]),
        encode(&[0xff; 64]),
        encode(&[1; 63]),
        encode(&[1; 65]),
        format!("{good}="),
        "A".repeat(100_000),
    ] {
        let challenge = sessions.challenge(&id, T0, &devices).unwrap();
        let error = sessions
            .verify(&challenge.id, &bad, T0, &devices)
            .unwrap_err();
        assert!(
            matches!(error, Error::InvalidSignature),
            "{bad:.20}: {error:?}"
        );
    }
}

#[test]
fn a_session_lasts_twelve_hours() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(1);
    let device = pair(&mut auth, &key, T0);
    let session = sign_in(&mut auth, &key, &device, T0);
    assert_eq!(
        auth.authenticate(&session.token, T0 + SESSION_TTL_MS - 1),
        Some(device)
    );
    assert_eq!(auth.authenticate(&session.token, T0 + SESSION_TTL_MS), None);
    // Gone for good, even at an earlier clock.
    assert_eq!(auth.authenticate(&session.token, T0), None);
}

#[test]
fn signing_in_marks_the_device_seen() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(1);
    let device = pair(&mut auth, &key, T0);
    sign_in(&mut auth, &key, &device, T0 + 5);
    assert_eq!(
        Devices::load(dir.path())
            .unwrap()
            .get(&device)
            .unwrap()
            .last_seen_ms,
        Some(T0 + 5)
    );
}

#[test]
fn revoking_ends_the_devices_sessions_and_challenges() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(1);
    let other_key = signing_key(2);
    let device = pair(&mut auth, &key, T0);
    let other = pair(&mut auth, &other_key, T0);
    let session = sign_in(&mut auth, &key, &device, T0);
    let kept = sign_in(&mut auth, &other_key, &other, T0);
    let client = IpAddr::from([192, 168, 1, 30]);
    let pending = auth.challenge(client, &device, T0).unwrap();

    assert!(auth.revoke(&device).unwrap());
    assert_eq!(auth.authenticate(&session.token, T0), None);
    assert_eq!(auth.authenticate(&kept.token, T0), Some(other));
    let error = auth
        .verify(&pending.id, &sign(&key, &pending.nonce), T0)
        .unwrap_err();
    assert!(matches!(error, Error::UnknownChallenge));
    assert!(matches!(
        auth.challenge(client, &device, T0).unwrap_err(),
        Error::UnknownDevice
    ));
    assert!(!auth.revoke(&device).unwrap());
}

#[test]
fn malformed_or_unknown_bearers_authenticate_nobody() {
    let mut sessions = Sessions::default();
    for bearer in ["", "!!!", &encode(&[0; 32]), &"A".repeat(10_000)] {
        assert_eq!(sessions.authenticate(bearer, T0), None);
    }
}

#[test]
fn challenges_are_rate_limited_per_address() {
    let dir = tempfile::tempdir().unwrap();
    let limits = Limits {
        challenge_burst: 2,
        challenge_refill_ms: 1_000,
        ..Limits::default()
    };
    let mut auth = Auth::load(dir.path(), limits).unwrap();
    let device = pair(&mut auth, &signing_key(1), T0);
    let client = IpAddr::from([10, 0, 0, 1]);
    auth.challenge(client, &device, T0).unwrap();
    auth.challenge(client, &device, T0).unwrap();
    assert!(matches!(
        auth.challenge(client, &device, T0).unwrap_err(),
        Error::RateLimited
    ));
    auth.challenge(IpAddr::from([10, 0, 0, 2]), &device, T0)
        .unwrap();
    auth.challenge(client, &device, T0 + 1_000).unwrap();
}
