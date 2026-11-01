//! Presence through the control socket, as Herdr UI relays it from the
//! `herdr-nearby` helper: a challenge signed by this computer's CA, the
//! phone's proof bound to that CA, the addresses sealed to it, and the near
//! mark that holds its alerts. Also writes the fixture the phone's Swift
//! tests read (`Fixtures/presence/prove.json`).

use super::*;
use crate::{
    apns::seal::{ADDRESSES_INFO, seal_with},
    auth::presence::{challenge_message, hint, message},
    control::{ErrorCode, Request, Response},
};
use aes_gcm::{
    Aes256Gcm, KeyInit as _,
    aead::{Aead as _, Payload},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use p256::{
    PublicKey, SecretKey,
    ecdsa::{Signature, SigningKey, signature::Signer},
};
use serde_json::{Value, json};
use sha2::Sha256;

fn control(state: &Shared, request: Request) -> Response {
    let (stop, _) = tokio::sync::watch::channel(false);
    crate::server::answer_for_tests(state, &stop, request)
}

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The key `signed_in` pairs with for `seed`.
fn device_key(seed: u8) -> SigningKey {
    SigningKey::from_slice(&[seed; 32]).unwrap()
}

const DAY_MS: u64 = 86_400_000;

/// This computer's CA fingerprint as raw bytes.
fn ca(state: &Shared) -> [u8; 32] {
    auth::nearby::fingerprint_bytes(&state.fingerprint).unwrap()
}

fn prove(key: &SigningKey, fingerprint: &[u8; 32], nonce: &str) -> String {
    let nonce = URL_SAFE_NO_PAD.decode(nonce).unwrap();
    let signature: Signature = key.sign(&message(fingerprint, &nonce));
    b64(&signature.to_bytes())
}

/// A hello for `device` today, and the challenge it gets.
fn challenge(state: &Shared, device: &DeviceId) -> (String, String, String) {
    let hint = b64(&hint(device, now_ms() / DAY_MS));
    match control(state, Request::PresenceChallenge { hint }) {
        Response::PresenceChallenge {
            certificate,
            nonce,
            signature,
        } => (certificate, nonce, signature),
        other => panic!("no challenge: {other:?}"),
    }
}

/// The P-256 key in a DER certificate, as the phone reads it.
fn certificate_key(der: &[u8]) -> p256::ecdsa::VerifyingKey {
    let (_, certificate) = x509_parser::parse_x509_certificate(der).unwrap();
    p256::ecdsa::VerifyingKey::from_sec1_bytes(&certificate.public_key().subject_public_key.data)
        .unwrap()
}

/// What the phone checks before it signs: the certificate is the one it
/// pinned, and it signed this challenge for this device.
fn phone_checks(
    fingerprint: &[u8; 32],
    device: &DeviceId,
    certificate: &str,
    nonce: &str,
    signature: &str,
) -> bool {
    use p256::ecdsa::signature::Verifier as _;
    let certificate = URL_SAFE_NO_PAD.decode(certificate).unwrap();
    let nonce = URL_SAFE_NO_PAD.decode(nonce).unwrap();
    let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(signature).unwrap()).unwrap();
    auth::sha256(&[&certificate]) == *fingerprint
        && certificate_key(&certificate)
            .verify(&challenge_message(&nonce, device), &signature)
            .is_ok()
}

/// Opens addresses sealed to `device` as the phone does.
fn open(device: &SecretKey, sealed: &str) -> Vec<u8> {
    let sealed = URL_SAFE_NO_PAD.decode(sealed).unwrap();
    let ephemeral = PublicKey::from_sec1_bytes(&sealed[..65]).unwrap();
    let mut salt = sealed[..65].to_vec();
    salt.extend_from_slice(&device.public_key().to_sec1_bytes());
    let shared = p256::ecdh::diffie_hellman(device.to_nonzero_scalar(), ephemeral.as_affine());
    let mut key = [0; 32];
    Hkdf::<Sha256>::new(Some(&salt), shared.raw_secret_bytes())
        .expand(ADDRESSES_INFO, &mut key)
        .unwrap();
    let nonce: [u8; 12] = sealed[65..77].try_into().unwrap();
    Aes256Gcm::new_from_slice(&key)
        .unwrap()
        .decrypt(
            (&nonce).into(),
            Payload {
                msg: &sealed[77..],
                aad: ADDRESSES_INFO,
            },
        )
        .unwrap()
}

#[test]
fn a_proven_phone_gets_this_computers_addresses_sealed_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (device, _) = signed_in(&state, 4);
    let notify = SecretKey::from_slice(&[6; 32]).unwrap();
    state
        .auth()
        .devices_mut()
        .set_apns(
            &device,
            Some(json!({ "notify_key": b64(&notify.public_key().to_sec1_bytes()) })),
        )
        .unwrap();

    let (certificate, nonce, signature) = challenge(&state, &device);
    assert!(phone_checks(
        &ca(&state),
        &device,
        &certificate,
        &nonce,
        &signature
    ));
    let proven = control(
        &state,
        Request::PresenceProve {
            signature: prove(&device_key(4), &ca(&state), &nonce),
            nonce,
        },
    );
    let Response::PresenceProven {
        device: proven_device,
        addresses: Some(addresses),
    } = proven
    else {
        panic!("not proven: {proven:?}");
    };
    assert_eq!(proven_device, device);
    let opened: Value = serde_json::from_slice(&open(&notify, &addresses)).unwrap();
    assert_eq!(opened, json!({ "origins": state.origins() }));
}

#[test]
fn the_challenge_is_signed_for_the_device_it_names() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (device, _) = signed_in(&state, 4);
    let (other, _) = signed_in(&state, 5);
    let (certificate, nonce, signature) = challenge(&state, &device);
    // A challenge the computer signed for one device is not one for another,
    // and another CA's fingerprint does not pin it.
    assert!(!phone_checks(
        &ca(&state),
        &other,
        &certificate,
        &nonce,
        &signature
    ));
    assert!(!phone_checks(
        &[1; 32],
        &device,
        &certificate,
        &nonce,
        &signature
    ));
}

#[test]
fn a_phone_without_a_notify_key_is_proven_without_addresses() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (device, _) = signed_in(&state, 4);
    let (_, nonce, _) = challenge(&state, &device);
    let proven = control(
        &state,
        Request::PresenceProve {
            signature: prove(&device_key(4), &ca(&state), &nonce),
            nonce,
        },
    );
    assert!(matches!(
        proven,
        Response::PresenceProven {
            addresses: None,
            ..
        }
    ));
}

#[test]
fn unknown_hints_and_bad_proofs_read_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let unknown = control(
        &state,
        Request::PresenceChallenge {
            hint: b64(&[0; 16]),
        },
    );
    let (device, _) = signed_in(&state, 4);
    let (_, nonce, _) = challenge(&state, &device);
    let bad = control(
        &state,
        Request::PresenceProve {
            signature: prove(&device_key(5), &ca(&state), &nonce),
            nonce,
        },
    );
    let (
        Response::Error { code, message },
        Response::Error {
            code: bad_code,
            message: bad_message,
        },
    ) = (unknown, bad)
    else {
        panic!("both refused");
    };
    assert_eq!((code, message), (bad_code, bad_message));
    assert!(matches!(
        control(
            &state,
            Request::PresenceSet {
                device: DeviceId("nobody".into()),
                near: true
            }
        ),
        Response::Error {
            code: ErrorCode::UnknownDevice,
            ..
        }
    ));
}

#[test]
fn a_near_mark_is_set_and_lifted() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (device, _) = signed_in(&state, 4);
    let set = |near| {
        control(
            &state,
            Request::PresenceSet {
                device: device.clone(),
                near,
            },
        )
    };
    assert_eq!(set(true), Response::Done);
    assert!(state.near().is_near(&device, now_ms()));
    assert_eq!(set(false), Response::Done);
    assert!(!state.near().is_near(&device, now_ms()));
}

/// Both sides from fixed values: the computer's hello hint, its CA and
/// challenge signature, what the phone signs, a signature the daemon's
/// verifier accepts, and addresses sealed to its notify key. The CA
/// certificate's own signature is randomized, so it and the challenge
/// signature are checked by what they prove rather than byte for byte.
#[test]
fn the_presence_fixture_matches() {
    use p256::ecdsa::signature::Verifier as _;
    let device_id = DeviceId("3TjaYoW0bNYChLkxfCexSw".into());
    let hint_day = 20_000u64;
    let device = device_key(4);
    let nonce = [3u8; 32];
    let ca_key = SigningKey::from_slice(&[8; 32]).unwrap();
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ios/HerdrRemoteCore/Tests/HerdrRemoteCoreTests/Fixtures/presence/prove.json");
    let update = std::env::var_os("HERDR_UPDATE_FIXTURES").is_some();
    let stored: Option<Value> = (!update).then(|| {
        serde_json::from_str(
            &std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display())),
        )
        .unwrap()
    });
    let ca_certificate = match &stored {
        Some(stored) => URL_SAFE_NO_PAD
            .decode(stored["ca_certificate"].as_str().unwrap())
            .unwrap(),
        None => fixture_ca(&ca_key),
    };
    // The certificate holds the fixed CA key, and pins as its digest.
    assert_eq!(
        certificate_key(&ca_certificate)
            .to_sec1_point(false)
            .as_bytes(),
        ca_key.verifying_key().to_sec1_point(false).as_bytes()
    );
    let fingerprint = auth::sha256(&[&ca_certificate]);
    let challenge_signature: Signature = ca_key.sign(&challenge_message(&nonce, &device_id));
    let signed = message(&fingerprint, &nonce);
    let signature: Signature = device.sign(&signed);
    // As `Presences::prove` checks it.
    let public =
        auth::PublicKey::parse(&b64(device.verifying_key().to_sec1_point(false).as_bytes()))
            .unwrap();
    public
        .verifying_key()
        .unwrap()
        .verify(&signed, &signature)
        .unwrap();
    let notify = SecretKey::from_slice(&[6; 32]).unwrap();
    let plaintext = json!({ "origins": ["https://192.168.1.20:7443", "https://100.64.0.3:7443"] });
    let sealed = b64(&seal_with(
        &notify.public_key(),
        plaintext.to_string().as_bytes(),
        ADDRESSES_INFO,
        &SecretKey::from_slice(&[9; 32]).unwrap(),
        &[1; 12],
    )
    .unwrap());
    assert_eq!(
        serde_json::from_slice::<Value>(&open(&notify, &sealed)).unwrap(),
        plaintext
    );
    let fixture = json!({
        "device_id": device_id.0,
        "hint_day": hint_day,
        "hint": b64(&hint(&device_id, hint_day)),
        "ca_certificate": b64(&ca_certificate),
        "ca_fingerprint": b64(&fingerprint),
        "challenge_message": b64(&challenge_message(&nonce, &device_id)),
        "challenge_signature": b64(&challenge_signature.to_bytes()),
        "device_private_key": b64(&device.to_bytes()),
        "device_public_key": b64(device.verifying_key().to_sec1_point(false).as_bytes()),
        "nonce": b64(&nonce),
        "message": b64(&signed),
        "signature": b64(&signature.to_bytes()),
        "addresses": {
            "device_notify_private_key": b64(&notify.to_bytes()),
            "plaintext": plaintext,
            "sealed": sealed,
        },
    });
    let Some(stored) = stored else {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&fixture).unwrap()),
        )
        .unwrap();
        return;
    };
    assert_eq!(
        fixture,
        stored,
        "{} is out of date; run with HERDR_UPDATE_FIXTURES=1",
        path.display()
    );
}

/// A CA certificate for the fixed key, as the daemon's own are made.
fn fixture_ca(key: &SigningKey) -> Vec<u8> {
    use p256::pkcs8::{EncodePrivateKey as _, LineEnding};
    let pem = SecretKey::from(key).to_pkcs8_pem(LineEnding::LF).unwrap();
    let key = rcgen::KeyPair::from_pem(&pem).unwrap();
    let mut params = rcgen::CertificateParams::default();
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Constrained(0));
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Herdr Remote fixture CA");
    params.self_signed(&key).unwrap().der().to_vec()
}
