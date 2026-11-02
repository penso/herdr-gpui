//! Presence proofs: a phone says hello with its daily hint, then signs a
//! fresh nonce bound to this computer's CA fingerprint, once, within
//! thirty seconds.

use super::*;
use crate::auth::presence::{MAX_PENDING_PER_DEVICE, NONCE_TTL_MS, hint, message};

/// The CA fingerprint the proofs here are bound to.
const CA: [u8; 32] = [7; 32];
const DAY_MS: u64 = 86_400_000;

fn hello(device: &DeviceId, now_ms: u64) -> String {
    encode(&hint(device, now_ms / DAY_MS))
}

fn sign_presence(key: &SigningKey, fingerprint: &[u8; 32], nonce: &[u8]) -> String {
    let signature: Signature = key.sign(&message(fingerprint, nonce));
    encode(&signature.to_bytes())
}

#[test]
fn a_paired_phone_proves_itself_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(3);
    let device = pair(&mut auth, &key, T0);
    let challenge = auth.presence_challenge(&hello(&device, T0), T0).unwrap();
    assert_eq!(challenge.device, device);
    let nonce = encode(&challenge.nonce);
    let signature = sign_presence(&key, &CA, &challenge.nonce);
    let proven = auth
        .presence_prove(&nonce, &signature, &CA, T0 + 10)
        .unwrap();
    assert_eq!(proven, device);
    let again = auth
        .presence_prove(&nonce, &signature, &CA, T0 + 20)
        .unwrap_err();
    assert!(matches!(again, Error::UnknownPresence));
}

#[test]
fn a_hint_matches_yesterday_today_and_tomorrow_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let device = pair(&mut auth, &signing_key(3), T0);
    let today = T0 / DAY_MS;
    for day in [today - 1, today, today + 1] {
        let challenge = auth
            .presence_challenge(&encode(&hint(&device, day)), T0)
            .unwrap();
        assert_eq!(challenge.device, device, "day {day}");
    }
    for day in [today - 2, today + 2] {
        let error = auth
            .presence_challenge(&encode(&hint(&device, day)), T0)
            .unwrap_err();
        assert!(matches!(error, Error::UnknownPresence), "day {day}");
    }
    // The hint names the device without carrying its id.
    assert!(!encode(&hint(&device, today)).contains(&device.0));
}

#[test]
fn a_proof_for_another_computer_or_a_late_answer_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(3);
    let device = pair(&mut auth, &key, T0);

    let challenge = auth.presence_challenge(&hello(&device, T0), T0).unwrap();
    let other_computer = sign_presence(&key, &[8; 32], &challenge.nonce);
    let error = auth
        .presence_prove(&encode(&challenge.nonce), &other_computer, &CA, T0)
        .unwrap_err();
    assert!(matches!(error, Error::UnknownPresence));

    let challenge = auth.presence_challenge(&hello(&device, T0), T0).unwrap();
    let late = auth
        .presence_prove(
            &encode(&challenge.nonce),
            &sign_presence(&key, &CA, &challenge.nonce),
            &CA,
            T0 + NONCE_TTL_MS,
        )
        .unwrap_err();
    assert!(matches!(late, Error::UnknownPresence));

    // A nonce nobody gave out.
    let made_up = [9; 32];
    let error = auth
        .presence_prove(
            &encode(&made_up),
            &sign_presence(&key, &CA, &made_up),
            &CA,
            T0,
        )
        .unwrap_err();
    assert!(matches!(error, Error::UnknownPresence));
}

#[test]
fn another_key_or_a_session_signature_does_not_prove_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(3);
    let device = pair(&mut auth, &key, T0);

    let challenge = auth.presence_challenge(&hello(&device, T0), T0).unwrap();
    let error = auth
        .presence_prove(
            &encode(&challenge.nonce),
            &sign_presence(&signing_key(4), &CA, &challenge.nonce),
            &CA,
            T0,
        )
        .unwrap_err();
    assert!(matches!(error, Error::UnknownPresence));

    // The same nonce signed as a session challenge means nothing here.
    let challenge = auth.presence_challenge(&hello(&device, T0), T0).unwrap();
    let nonce = encode(&challenge.nonce);
    let error = auth
        .presence_prove(&nonce, &sign(&key, &nonce), &CA, T0)
        .unwrap_err();
    assert!(matches!(error, Error::UnknownPresence));
}

#[test]
fn forged_hellos_do_not_cancel_the_real_phones_challenge_until_the_cap() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(3);
    let device = pair(&mut auth, &key, T0);
    let real = auth.presence_challenge(&hello(&device, T0), T0).unwrap();
    // Someone who learned today's hint says hello again and again.
    for _ in 1..MAX_PENDING_PER_DEVICE {
        auth.presence_challenge(&hello(&device, T0), T0).unwrap();
    }
    auth.presence_prove(
        &encode(&real.nonce),
        &sign_presence(&key, &CA, &real.nonce),
        &CA,
        T0,
    )
    .unwrap();

    // Past the cap the oldest goes first.
    let oldest = auth.presence_challenge(&hello(&device, T0), T0).unwrap();
    for _ in 0..MAX_PENDING_PER_DEVICE {
        auth.presence_challenge(&hello(&device, T0), T0).unwrap();
    }
    let error = auth
        .presence_prove(
            &encode(&oldest.nonce),
            &sign_presence(&key, &CA, &oldest.nonce),
            &CA,
            T0,
        )
        .unwrap_err();
    assert!(matches!(error, Error::UnknownPresence));
}

#[test]
fn every_failure_reads_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(3);
    let device = pair(&mut auth, &key, T0);
    let unknown_hint = auth
        .presence_challenge(&encode(&[0; 16]), T0)
        .unwrap_err()
        .to_string();
    let challenge = auth.presence_challenge(&hello(&device, T0), T0).unwrap();
    let bad_signature = auth
        .presence_prove(
            &encode(&challenge.nonce),
            &sign_presence(&signing_key(4), &CA, &challenge.nonce),
            &CA,
            T0,
        )
        .unwrap_err()
        .to_string();
    let malformed = auth
        .presence_challenge("not base64!", T0)
        .unwrap_err()
        .to_string();
    assert_eq!(unknown_hint, bad_signature);
    assert_eq!(unknown_hint, malformed);
}
