//! The Home Screen app takes over its browser's device with a single-use
//! token and a key of its own.

use super::*;

const CLIENT: [u8; 4] = [192, 168, 1, 30];

#[test]
fn a_redeemed_handoff_moves_the_device_to_the_new_key() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let browser = signing_key(1);
    let device = pair(&mut auth, &browser, T0);
    let session = sign_in(&mut auth, &browser, &device, T0);
    let handoff = auth.handoff(&device, T0).unwrap();
    assert_eq!(
        handoff.expires_ms,
        T0 + crate::auth::handoff::HANDOFF_TTL_MS
    );

    let app = signing_key(2);
    let redeemed = auth
        .redeem(
            IpAddr::from(CLIENT),
            &handoff.token,
            &public_key(&app),
            T0 + 1,
        )
        .unwrap();
    assert_eq!(redeemed, device);
    // One device still, under the same name, now signing with the app's key.
    assert_eq!(auth.devices().len(), 1);
    assert_eq!(auth.devices().get(&device).unwrap().name, "iPhone");
    assert!(
        auth.authenticate(&session.token, T0 + 2).is_none(),
        "the browser's session ended"
    );
    sign_in(&mut auth, &app, &device, T0 + 3);
    let challenge = auth
        .challenge(IpAddr::from(CLIENT), &device, T0 + 4)
        .unwrap();
    assert!(matches!(
        auth.verify(&challenge.id, &sign(&browser, &challenge.nonce), T0 + 4),
        Err(Error::InvalidSignature)
    ));
    // The new key survives a restart.
    let mut reloaded = super::auth(dir.path());
    sign_in(&mut reloaded, &app, &device, T0 + 5);
}

#[test]
fn a_handoff_is_used_once_and_expires() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let device = pair(&mut auth, &signing_key(1), T0);
    let client = IpAddr::from(CLIENT);
    let handoff = auth.handoff(&device, T0).unwrap();
    auth.redeem(client, &handoff.token, &public_key(&signing_key(2)), T0)
        .unwrap();
    assert!(matches!(
        auth.redeem(client, &handoff.token, &public_key(&signing_key(3)), T0),
        Err(Error::InvalidToken)
    ));
    let late = auth.handoff(&device, T0).unwrap();
    assert!(matches!(
        auth.redeem(
            client,
            &late.token,
            &public_key(&signing_key(3)),
            T0 + crate::auth::handoff::HANDOFF_TTL_MS
        ),
        Err(Error::InvalidToken)
    ));
}

#[test]
fn minting_again_voids_the_previous_token() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let device = pair(&mut auth, &signing_key(1), T0);
    let first = auth.handoff(&device, T0).unwrap();
    let second = auth.handoff(&device, T0 + 1).unwrap();
    let client = IpAddr::from(CLIENT);
    assert!(matches!(
        auth.redeem(client, &first.token, &public_key(&signing_key(2)), T0 + 2),
        Err(Error::InvalidToken)
    ));
    auth.redeem(client, &second.token, &public_key(&signing_key(2)), T0 + 2)
        .unwrap();
}

#[test]
fn only_a_paired_device_hands_off_and_revoking_voids_its_token() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    assert!(matches!(
        auth.handoff(&DeviceId("nobody".into()), T0),
        Err(Error::UnknownDevice)
    ));
    let device = pair(&mut auth, &signing_key(1), T0);
    let handoff = auth.handoff(&device, T0).unwrap();
    auth.revoke(&device).unwrap();
    assert!(matches!(
        auth.redeem(
            IpAddr::from(CLIENT),
            &handoff.token,
            &public_key(&signing_key(2)),
            T0
        ),
        Err(Error::InvalidToken)
    ));
}

#[test]
fn a_bad_key_leaves_the_token_usable() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let device = pair(&mut auth, &signing_key(1), T0);
    let handoff = auth.handoff(&device, T0).unwrap();
    let client = IpAddr::from(CLIENT);
    assert!(matches!(
        auth.redeem(client, &handoff.token, "not-a-key", T0),
        Err(Error::InvalidKey)
    ));
    auth.redeem(client, &handoff.token, &public_key(&signing_key(2)), T0)
        .unwrap();
}

#[test]
fn a_handoff_survives_a_restart_and_stays_used_after_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth_before = auth(dir.path());
    let browser = signing_key(1);
    let device = pair(&mut auth_before, &browser, T0);
    let handoff = auth_before.handoff(&device, T0).unwrap();
    drop(auth_before);

    let app = signing_key(2);
    let mut restarted = auth(dir.path());
    let redeemed = restarted
        .redeem(
            IpAddr::from(CLIENT),
            &handoff.token,
            &public_key(&app),
            T0 + 1,
        )
        .unwrap();
    assert_eq!(redeemed, device);
    drop(restarted);

    let mut again = auth(dir.path());
    assert!(matches!(
        again.redeem(
            IpAddr::from(CLIENT),
            &handoff.token,
            &public_key(&signing_key(3)),
            T0 + 2
        ),
        Err(Error::InvalidToken)
    ));
    let saved = std::fs::read_to_string(dir.path().join("handoffs.json")).unwrap();
    assert!(!saved.contains(&handoff.token));
}
