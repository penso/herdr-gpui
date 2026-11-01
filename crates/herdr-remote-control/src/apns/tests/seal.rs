//! The sealed format must match what the Swift package opens; the fixture
//! fixes both one-time values so the bytes are reproducible.

use crate::apns::seal::{INFO, seal_with};
use aes_gcm::{Aes256Gcm, KeyInit as _, aead::Aead as _, aead::Payload};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use p256::{PublicKey, SecretKey};
use serde_json::Value;
use sha2::Sha256;

const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../ios/HerdrRemoteCore/Tests/HerdrRemoteCoreTests/Fixtures/seal.json"
));

fn bytes(value: &Value, field: &str) -> Vec<u8> {
    URL_SAFE_NO_PAD
        .decode(value[field].as_str().expect("field is a string"))
        .expect("field is base64url")
}

#[test]
fn seals_the_fixture_bytes() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let device = SecretKey::from_slice(&bytes(&fixture, "device_private_key")).unwrap();
    let device_public = PublicKey::from_sec1_bytes(&bytes(&fixture, "device_public_key")).unwrap();
    assert_eq!(device.public_key(), device_public);
    let ephemeral = SecretKey::from_slice(&bytes(&fixture, "ephemeral_private_key")).unwrap();
    let nonce: [u8; 12] = bytes(&fixture, "nonce").try_into().unwrap();
    let plaintext = fixture["plaintext"].as_str().unwrap();

    let sealed = seal_with(
        &device_public,
        plaintext.as_bytes(),
        INFO,
        &ephemeral,
        &nonce,
    )
    .unwrap();

    assert_eq!(
        URL_SAFE_NO_PAD.encode(&sealed),
        fixture["sealed"].as_str().unwrap()
    );
}

/// Opens as the phone does, from the device's private key alone.
#[test]
fn the_device_key_opens_it_and_a_changed_byte_does_not() {
    let device = SecretKey::from_slice(&[7; 32]).unwrap();
    let ephemeral = SecretKey::from_slice(&[9; 32]).unwrap();
    let sealed = seal_with(&device.public_key(), b"hello", INFO, &ephemeral, &[1; 12]).unwrap();

    let open = |sealed: &[u8]| -> Option<Vec<u8>> {
        let ephemeral = PublicKey::from_sec1_bytes(&sealed[..65]).ok()?;
        let mut salt = sealed[..65].to_vec();
        salt.extend_from_slice(&device.public_key().to_sec1_bytes());
        let shared = p256::ecdh::diffie_hellman(device.to_nonzero_scalar(), ephemeral.as_affine());
        let mut key = [0; 32];
        Hkdf::<Sha256>::new(Some(&salt), shared.raw_secret_bytes())
            .expand(INFO, &mut key)
            .ok()?;
        let nonce: [u8; 12] = sealed[65..77].try_into().ok()?;
        Aes256Gcm::new_from_slice(&key)
            .ok()?
            .decrypt(
                (&nonce).into(),
                Payload {
                    msg: &sealed[77..],
                    aad: INFO,
                },
            )
            .ok()
    };
    assert_eq!(open(&sealed).as_deref(), Some(&b"hello"[..]));
    let mut tampered = sealed;
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    assert_eq!(open(&tampered), None);
}
