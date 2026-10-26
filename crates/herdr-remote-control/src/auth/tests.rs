use super::*;
use p256::ecdsa::{Signature, SigningKey, signature::Signer};

mod devices;
mod handoff;
mod inputs;
mod limits;
mod pairing;
mod session;

const ORIGIN: &str = "https://192.168.1.20:7443";
const FINGERPRINT: &str = "AB:CD:EF:01";
const T0: u64 = 1_700_000_000_000;

fn origins() -> Vec<String> {
    vec![ORIGIN.to_owned(), "https://100.64.0.3:7443".to_owned()]
}

/// A deterministic device key; `seed` must be nonzero.
fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_slice(&[seed; 32]).unwrap()
}

fn public_key(key: &SigningKey) -> String {
    encode(key.verifying_key().to_sec1_point(false).as_bytes())
}

/// Signs a challenge nonce the way the phone's WebCrypto key does: ECDSA
/// P-256 with SHA-256, IEEE P1363 (r || s).
fn sign(key: &SigningKey, nonce: &str) -> String {
    let nonce: [u8; 32] = decode_fixed(nonce).unwrap();
    let signature: Signature = key.sign(&super::session::signed_message(&nonce));
    encode(&signature.to_bytes())
}

/// The token in an offer's URL fragment.
fn token(offer: &Offer) -> String {
    let fragment = offer.url.split_once("#t=").unwrap().1;
    fragment.split_once('&').unwrap().0.to_owned()
}

fn new_device(seed: u8, name: &str) -> NewDevice {
    NewDevice {
        app: None,
        id: DeviceId(format!("device-{seed}")),
        name: name.to_owned(),
        public_key: PublicKey::parse(&public_key(&signing_key(seed))).unwrap(),
    }
}

/// A device store that keeps everything, for pairing alone.
fn keep(_: &NewDevice) -> Result<()> {
    Ok(())
}

fn auth(dir: &Path) -> Auth {
    Auth::load(dir, Limits::default()).unwrap()
}

/// Pairs a device through the façade and returns its id.
fn pair(auth: &mut Auth, key: &SigningKey, now_ms: u64) -> DeviceId {
    let offer = auth.mint(now_ms, &origins(), FINGERPRINT).unwrap();
    let claim = auth
        .claim(
            IpAddr::from([192, 168, 1, 30]),
            &token(&offer),
            "iPhone",
            &public_key(key),
            now_ms,
        )
        .unwrap();
    match auth.answer(&offer.id, true, &claim.code, now_ms).unwrap() {
        PairingState::Approved { device } => device,
        state => panic!("not approved: {state:?}"),
    }
}

/// Starts a session for a paired device through the façade.
fn sign_in(auth: &mut Auth, key: &SigningKey, device: &DeviceId, now_ms: u64) -> Session {
    let client = IpAddr::from([192, 168, 1, 30]);
    let challenge = auth.challenge(client, device, now_ms).unwrap();
    auth.verify(&challenge.id, &sign(key, &challenge.nonce), now_ms)
        .unwrap()
}
