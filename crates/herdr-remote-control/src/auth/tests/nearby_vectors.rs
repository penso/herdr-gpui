//! The nearby pairing vectors the Swift package checks: every key and
//! nonce fixed, and what each step must produce. `HERDR_UPDATE_FIXTURES=1`
//! rewrites them.

use super::nearby::{FP, Phone};
use super::*;
use crate::auth::nearby::{
    Exchange, code, commit, fingerprint_bytes, fingerprint_hex, key, shared_secret, transcript,
};
use p256::SecretKey;
use serde_json::{Value, json};
use std::path::PathBuf;

const TOKEN: [u8; 32] = [0x7a; 32];
const AES_NONCE: [u8; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

fn check(name: &str, value: &Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ios/HerdrRemoteCore/Tests/HerdrRemoteCoreTests/Fixtures/nearby")
        .join(format!("{name}.json"));
    let json = format!("{}\n", serde_json::to_string_pretty(value).unwrap());
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, json).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    // Compared as values: the file's key order follows whether the build
    // turned on serde_json's `preserve_order`, its contents do not.
    let expected: Value = serde_json::from_str(&expected).unwrap();
    assert_eq!(
        value,
        &expected,
        "{} is out of date; run with HERDR_UPDATE_FIXTURES=1",
        path.display()
    );
}

fn point(secret: &SecretKey) -> Vec<u8> {
    secret.public_key().to_sec1_bytes().to_vec()
}

/// One leg of the exchange from fixed values: what the daemon sends and
/// what both sides compute.
fn leg(phone: &Phone, daemon: &SecretKey, daemon_nonce: [u8; 32], fingerprint: &str) -> Value {
    let fp = fingerprint_bytes(fingerprint).unwrap();
    let phone_key = PublicKey::parse(&phone.public_key()).unwrap();
    let transcript = transcript(
        phone_key.as_bytes(),
        &point(daemon),
        &phone.nonce,
        &daemon_nonce,
        &fp,
    );
    let shared = shared_secret(daemon, &phone_key).unwrap();
    let revealed = Exchange {
        daemon,
        phone: &phone_key,
        phone_nonce: &phone.nonce,
        daemon_nonce: &daemon_nonce,
        fingerprint: &fp,
    }
    .reveal(&TOKEN, &origins(), &AES_NONCE)
    .unwrap();
    json!({
        "phone_private_key": encode(&phone.secret.to_bytes()),
        "phone_public_key": phone.public_key(),
        "phone_nonce": encode(&phone.nonce),
        "daemon_private_key": encode(&daemon.to_bytes()),
        "daemon_public_key": encode(&point(daemon)),
        "daemon_nonce": encode(&daemon_nonce),
        "fingerprint": fingerprint_hex(&fp),
        "commit": encode(&commit(&point(daemon), phone_key.as_bytes(), &daemon_nonce)),
        "code": code(&transcript),
        "key": encode(key(shared.as_slice(), &transcript).unwrap().as_slice()),
        "aes_nonce": encode(&AES_NONCE),
        "plaintext": serde_json::to_string(&crate::auth::nearby::Offer {
            token: encode(&TOKEN),
            origins: &origins(),
        })
        .unwrap(),
        "sealed": encode(&revealed.sealed),
    })
}

#[test]
fn a_session_matches_the_shared_vector() {
    let phone = Phone::new(0x21, 0x31);
    let daemon = SecretKey::from_slice(&[0x42; 32]).unwrap();
    let vector = leg(&phone, &daemon, [0x52; 32], FP);
    check("session", &vector);

    // The test phone, written from the spec, agrees with every field.
    let started = NearbyStarted {
        session: "s".into(),
        computer: "Studio".into(),
        fingerprint: vector["fingerprint"].as_str().unwrap().into(),
        public_key: vector["daemon_public_key"].as_str().unwrap().into(),
        commit: vector["commit"].as_str().unwrap().into(),
    };
    let revealed = NearbyRevealed {
        nonce: vector["daemon_nonce"].as_str().unwrap().into(),
        sealed: vector["sealed"].as_str().unwrap().into(),
    };
    let finished = phone.finish(&started, &revealed).unwrap();
    assert_eq!(finished.code, vector["code"].as_str().unwrap());
    assert_eq!(finished.token, encode(&TOKEN));
}

/// A man in the middle runs one exchange with the phone and another with
/// the daemon, each with its own keys and nonces. Committed to its nonce
/// before it sees the daemon's, it cannot steer the two codes together.
#[test]
fn a_relay_shows_different_codes_on_each_side() {
    let phone = Phone::new(0x21, 0x31);
    let daemon = SecretKey::from_slice(&[0x42; 32]).unwrap();
    let relay_as_daemon = SecretKey::from_slice(&[0x63; 32]).unwrap();
    let relay_as_phone = Phone::new(0x64, 0x74);
    let phone_side = leg(&phone, &relay_as_daemon, [0x73; 32], FP);
    let daemon_side = leg(&relay_as_phone, &daemon, [0x52; 32], FP);
    assert_ne!(phone_side["code"], daemon_side["code"]);
    check(
        "relay",
        &json!({
            "phone_side": phone_side,
            "daemon_side": daemon_side,
        }),
    );
}
