//! Nearby pairing through `Auth`, with a phone written from the spec:
//! it checks the commitment, derives the code and key, and opens the token.

use super::*;
use crate::auth::{
    nearby::{
        KEY_INFO, MAX_SESSIONS, SESSION_TTL_MS, code, commit, fingerprint_bytes, key, transcript,
    },
    pairing::{MAX_NEARBY_REVEALS, TOKEN_TTL_MS},
};
use aes_gcm::{
    Aes256Gcm, KeyInit as _,
    aead::{Aead as _, Payload},
};
use p256::SecretKey;

/// A CA fingerprint as the daemon writes it.
pub(super) const FP: &str = "9E:92:A0:DF:1C:E0:96:7E:C3:4C:0C:01:46:00:5F:BC:51:65:43:30:E7:D2:6E:C9:B5:80:50:55:75:5E:24:F6";

/// The phone's side of the exchange, from fixed values.
pub(super) struct Phone {
    pub(super) secret: SecretKey,
    pub(super) nonce: [u8; 32],
}

/// What the phone ends with: the code it shows, and the sealed contents.
#[derive(Debug, PartialEq)]
pub(super) struct Finished {
    pub(super) code: String,
    pub(super) token: String,
    pub(super) origins: Vec<String>,
}

impl Phone {
    pub(super) fn new(seed: u8, nonce: u8) -> Self {
        Self {
            secret: SecretKey::from_slice(&[seed; 32]).unwrap(),
            nonce: [nonce; 32],
        }
    }

    pub(super) fn public_key(&self) -> String {
        encode(&self.secret.public_key().to_sec1_bytes())
    }

    pub(super) fn nonce(&self) -> String {
        encode(&self.nonce)
    }

    /// Checks the daemon's commitment against its revealed nonce, then
    /// derives the code and opens the sealed token; `None` when either
    /// fails, as the phone would refuse.
    pub(super) fn finish(
        &self,
        started: &NearbyStarted,
        revealed: &NearbyRevealed,
    ) -> Option<Finished> {
        let daemon_key: [u8; 65] = decode_fixed(&started.public_key).ok()?;
        let daemon_nonce: [u8; 32] = decode_fixed(&revealed.nonce).ok()?;
        let phone_key = self.secret.public_key().to_sec1_bytes();
        let expected = encode(&commit(&daemon_key, &phone_key, &daemon_nonce));
        if !constant_time_eq(expected.as_bytes(), started.commit.as_bytes()) {
            return None;
        }
        let fingerprint = fingerprint_bytes(&started.fingerprint)?;
        let transcript = transcript(
            &phone_key,
            &daemon_key,
            &self.nonce,
            &daemon_nonce,
            &fingerprint,
        );
        let point = p256::PublicKey::from_sec1_bytes(&daemon_key).ok()?;
        let shared = p256::ecdh::diffie_hellman(self.secret.to_nonzero_scalar(), point.as_affine());
        let key = key(shared.raw_secret_bytes(), &transcript).ok()?;
        let sealed = URL_SAFE_NO_PAD.decode(&revealed.sealed).ok()?;
        let nonce: [u8; 12] = sealed.get(..12)?.try_into().ok()?;
        let plaintext = Aes256Gcm::new_from_slice(key.as_slice())
            .ok()?
            .decrypt(
                (&nonce).into(),
                Payload {
                    msg: &sealed[12..],
                    aad: KEY_INFO,
                },
            )
            .ok()?;
        let contents: serde_json::Value = serde_json::from_slice(&plaintext).ok()?;
        Some(Finished {
            code: code(&transcript),
            token: contents["token"].as_str()?.to_owned(),
            origins: serde_json::from_value(contents["origins"].clone()).ok()?,
        })
    }
}

/// An auth state with a pairing dialog open.
fn open(dir: &Path) -> (Auth, Offer) {
    let mut auth = auth(dir);
    let offer = auth.mint(T0, &origins(), FP).unwrap();
    (auth, offer)
}

fn exchange(auth: &mut Auth, phone: &Phone, now_ms: u64) -> (NearbyStarted, NearbyRevealed) {
    let started = auth
        .nearby_start(None, &phone.public_key(), "Studio", FP, now_ms)
        .unwrap();
    let revealed = auth
        .nearby_reveal(&started.session, &phone.nonce(), FP, &origins(), now_ms)
        .unwrap();
    (started, revealed)
}

#[test]
fn nothing_starts_while_no_pairing_dialog_is_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let error = auth
        .nearby_start(None, &Phone::new(3, 4).public_key(), "Studio", FP, T0)
        .unwrap_err();
    assert!(matches!(error, Error::NoPairing));
    assert!(!auth.pairing_open(T0));
}

#[test]
fn the_phone_and_the_desktop_show_the_same_code_and_the_claim_carries_it() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, offer) = open(dir.path());
    assert!(auth.pairing_open(T0));
    let phone = Phone::new(3, 4);
    let (started, revealed) = exchange(&mut auth, &phone, T0);
    assert_eq!(started.computer, "Studio");
    assert_eq!(started.fingerprint, FP.replace(':', ""));
    let finished = phone
        .finish(&started, &revealed)
        .expect("the phone accepts the exchange");
    assert_eq!(finished.origins, origins());

    let device_key = public_key(&signing_key(9));
    let claim = auth
        .claim(
            "10.0.0.9".parse().unwrap(),
            &finished.token,
            "iPhone",
            &device_key,
            T0 + 10,
        )
        .unwrap();
    assert_eq!(claim.code, finished.code);
    assert!(matches!(
        auth.pairing(&offer.id, T0 + 10).unwrap(),
        PairingState::Claimed { code, .. } if code == finished.code
    ));
    assert!(!auth.pairing_open(T0 + 10));
}

#[test]
fn a_nearby_token_is_claimed_once() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, _) = open(dir.path());
    let phone = Phone::new(3, 4);
    let (started, revealed) = exchange(&mut auth, &phone, T0);
    let token = phone.finish(&started, &revealed).unwrap().token;
    let key = public_key(&signing_key(9));
    auth.claim("10.0.0.9".parse().unwrap(), &token, "iPhone", &key, T0)
        .unwrap();
    let again = auth
        .claim("10.0.0.9".parse().unwrap(), &token, "iPhone", &key, T0)
        .unwrap_err();
    assert!(matches!(again, Error::InvalidToken));
}

#[test]
fn the_qr_token_still_pairs_with_its_own_code_beside_a_nearby_one() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, offer) = open(dir.path());
    let phone = Phone::new(3, 4);
    let (started, revealed) = exchange(&mut auth, &phone, T0);
    let nearby = phone.finish(&started, &revealed).unwrap();
    let claim = auth
        .claim(
            "10.0.0.9".parse().unwrap(),
            &token(&offer),
            "iPhone",
            &public_key(&signing_key(9)),
            T0,
        )
        .unwrap();
    assert_ne!(claim.code, nearby.code, "a QR claim derives its own code");
}

#[test]
fn rotating_the_qr_keeps_the_nearby_token_and_it_outlives_the_qr() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, offer) = open(dir.path());
    let phone = Phone::new(3, 4);
    let later = T0 + TOKEN_TTL_MS - 5;
    let (started, revealed) = exchange(&mut auth, &phone, later);
    let rotated = auth.mint(later + 1, &origins(), FP).unwrap();
    assert_eq!(rotated.id, offer.id);
    // The first QR token has expired; the nearby one has not.
    let token = phone.finish(&started, &revealed).unwrap().token;
    let claim = auth.claim(
        "10.0.0.9".parse().unwrap(),
        &token,
        "iPhone",
        &public_key(&signing_key(9)),
        T0 + TOKEN_TTL_MS + 10,
    );
    assert!(claim.is_ok(), "{claim:?}");
}

#[test]
fn a_nearby_token_expires() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, _) = open(dir.path());
    let phone = Phone::new(3, 4);
    let (started, revealed) = exchange(&mut auth, &phone, T0);
    let token = phone.finish(&started, &revealed).unwrap().token;
    let error = auth
        .claim(
            "10.0.0.9".parse().unwrap(),
            &token,
            "iPhone",
            &public_key(&signing_key(9)),
            T0 + TOKEN_TTL_MS,
        )
        .unwrap_err();
    assert!(matches!(error, Error::ExpiredToken));
}

#[test]
fn a_session_is_revealed_once_and_expires() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, _) = open(dir.path());
    let phone = Phone::new(3, 4);
    let started = auth
        .nearby_start(None, &phone.public_key(), "Studio", FP, T0)
        .unwrap();
    auth.nearby_reveal(&started.session, &phone.nonce(), FP, &origins(), T0)
        .unwrap();
    let twice = auth
        .nearby_reveal(&started.session, &phone.nonce(), FP, &origins(), T0)
        .unwrap_err();
    assert!(matches!(twice, Error::UnknownNearby));

    let late = auth
        .nearby_start(None, &phone.public_key(), "Studio", FP, T0)
        .unwrap();
    let expired = auth
        .nearby_reveal(
            &late.session,
            &phone.nonce(),
            FP,
            &origins(),
            T0 + SESSION_TTL_MS,
        )
        .unwrap_err();
    assert!(matches!(expired, Error::UnknownNearby));
    let unknown = auth
        .nearby_reveal("nope", &phone.nonce(), FP, &origins(), T0)
        .unwrap_err();
    assert!(matches!(unknown, Error::UnknownNearby));
}

#[test]
fn at_most_four_sessions_wait_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, _) = open(dir.path());
    let phone = Phone::new(3, 4);
    for _ in 0..MAX_SESSIONS {
        auth.nearby_start(None, &phone.public_key(), "Studio", FP, T0)
            .unwrap();
    }
    let busy = auth
        .nearby_start(None, &phone.public_key(), "Studio", FP, T0)
        .unwrap_err();
    assert!(matches!(busy, Error::NearbyBusy));
    // Expired sessions make room; the dialog has rotated its QR meanwhile.
    let later = T0 + SESSION_TTL_MS;
    auth.mint(later, &origins(), FP).unwrap();
    auth.nearby_start(None, &phone.public_key(), "Studio", FP, later)
        .unwrap();
}

#[test]
fn a_changed_daemon_nonce_fails_the_phones_commitment_check() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, _) = open(dir.path());
    let phone = Phone::new(3, 4);
    let (started, mut revealed) = exchange(&mut auth, &phone, T0);
    revealed.nonce = encode(&[0x55; 32]);
    assert_eq!(phone.finish(&started, &revealed), None);
}

#[test]
fn starting_over_the_network_is_rate_limited_per_address() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, _) = open(dir.path());
    let phone = Phone::new(3, 4);
    let client: IpAddr = "10.0.0.9".parse().unwrap();
    let mut refused = false;
    for _ in 0..(Limits::default().pair_burst + 1) {
        refused |= matches!(
            auth.nearby_start(Some(client), &phone.public_key(), "Studio", FP, T0),
            Err(Error::RateLimited)
        );
        // Keeps the session cap out of the way.
        auth.nearby = NearbySessions::default();
    }
    assert!(refused);
}

#[test]
fn a_bad_phone_key_or_nonce_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, _) = open(dir.path());
    let bad = auth
        .nearby_start(None, "AAAA", "Studio", FP, T0)
        .unwrap_err();
    assert!(matches!(bad, Error::InvalidKey));
    let phone = Phone::new(3, 4);
    let started = auth
        .nearby_start(None, &phone.public_key(), "Studio", FP, T0)
        .unwrap();
    let short = auth
        .nearby_reveal(&started.session, "AAAA", FP, &origins(), T0)
        .unwrap_err();
    assert!(matches!(short, Error::NearbyCrypto));
}

#[test]
fn one_pairing_seals_only_a_few_nearby_tokens_even_as_the_qr_rotates() {
    let dir = tempfile::tempdir().unwrap();
    let (mut auth, offer) = open(dir.path());
    let phone = Phone::new(3, 4);
    for attempt in 0..MAX_NEARBY_REVEALS {
        let now = T0 + u64::from(attempt) * 1_000;
        exchange(&mut auth, &phone, now);
        // The dialog rotates its QR; the count stays with the pairing.
        assert_eq!(auth.mint(now + 1, &origins(), FP).unwrap().id, offer.id);
    }
    let now = T0 + 10_000;
    let started = auth
        .nearby_start(None, &phone.public_key(), "Studio", FP, now)
        .unwrap();
    let refused = auth
        .nearby_reveal(&started.session, &phone.nonce(), FP, &origins(), now)
        .unwrap_err();
    assert!(matches!(refused, Error::NearbyExhausted));
    // The QR still pairs.
    assert!(auth.pairing_open(now));
}
