//! Nearby pairing: a phone that found this computer over Bluetooth or
//! Bonjour, with no QR to prove which computer answers, agrees on a
//! six-digit code with it (`docs/ios-app-plan.md`, "Nearby pairing,
//! exactly"). The daemon commits to its nonce before it sees the phone's,
//! so a man in the middle gets one guess at the code; the CA fingerprint
//! is part of what the code covers. The phone then claims a token sealed
//! under the agreed key, and the desktop dialog shows the same code.
//!
//! The functions taking explicit keys and nonces are what the shared
//! vectors check; [`NearbySessions`] holds the daemon's half between the
//! two steps.

use super::{
    Error, PublicKey, Result, SECRET_LEN, constant_time_eq, decode_fixed, encode, random_bytes,
    random_id, sha256,
};
use crate::control::{NearbyRevealed, NearbyStarted};
use aes_gcm::{
    Aes256Gcm, KeyInit as _,
    aead::{Aead as _, Payload},
};
use hkdf::Hkdf;
use p256::{SecretKey, elliptic_curve::zeroize::Zeroizing};
use sha2::Sha256;

/// Domain of the daemon's commitment.
pub(crate) const COMMIT_DOMAIN: &[u8] = b"herdr-sas-commit-v1";
/// Domain of the six-digit code.
pub(crate) const CODE_DOMAIN: &[u8] = b"herdr-sas-code-v1";
/// HKDF info of the sealing key, and the AES-GCM associated data.
pub(crate) const KEY_INFO: &[u8] = b"herdr-sas-key-v1";
/// How long a started session waits for the phone's nonce.
pub(crate) const SESSION_TTL_MS: u64 = 60_000;
/// Sessions held at once; a phone finishes its two steps in moments.
pub(crate) const MAX_SESSIONS: usize = 4;
const AES_NONCE_LEN: usize = 12;

/// `SHA-256(COMMIT_DOMAIN || PKb || PKa || Nb)`.
pub(crate) fn commit(daemon_key: &[u8], phone_key: &[u8], daemon_nonce: &[u8]) -> [u8; 32] {
    sha256(&[COMMIT_DOMAIN, daemon_key, phone_key, daemon_nonce])
}

/// `T = PKa || PKb || Na || Nb || fp`: everything both codes cover.
pub(crate) fn transcript(
    phone_key: &[u8],
    daemon_key: &[u8],
    phone_nonce: &[u8],
    daemon_nonce: &[u8],
    fingerprint: &[u8; 32],
) -> Vec<u8> {
    [
        phone_key,
        daemon_key,
        phone_nonce,
        daemon_nonce,
        fingerprint,
    ]
    .concat()
}

/// The six digits: the first four bytes of `SHA-256(CODE_DOMAIN || T)`,
/// big-endian, modulo one million.
pub(crate) fn code(transcript: &[u8]) -> String {
    let digest = sha256(&[CODE_DOMAIN, transcript]);
    let value = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    format!("{:06}", value % 1_000_000)
}

/// `HKDF-SHA256(ikm: ECDH secret, salt: SHA-256(T), info: KEY_INFO)`.
pub(crate) fn key(shared: &[u8], transcript: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    let mut key = Zeroizing::new([0; 32]);
    Hkdf::<Sha256>::new(Some(&sha256(&[transcript])), shared)
        .expand(KEY_INFO, key.as_mut_slice())
        .map_err(|_| Error::NearbyCrypto)?;
    Ok(key)
}

/// `nonce || AES-256-GCM(key, plaintext, aad: KEY_INFO)`.
pub(crate) fn seal(
    key: &[u8; 32],
    nonce: &[u8; AES_NONCE_LEN],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let sealed = Aes256Gcm::new_from_slice(key)
        .map_err(|_| Error::NearbyCrypto)?
        .encrypt(
            nonce.into(),
            Payload {
                msg: plaintext,
                aad: KEY_INFO,
            },
        )
        .map_err(|_| Error::NearbyCrypto)?;
    Ok([nonce.as_slice(), &sealed].concat())
}

/// The ECDH secret between the daemon's key and the phone's point.
pub(crate) fn shared_secret(daemon: &SecretKey, phone: &PublicKey) -> Result<Zeroizing<[u8; 32]>> {
    let point =
        p256::PublicKey::from_sec1_bytes(phone.as_bytes()).map_err(|_| Error::InvalidKey)?;
    let shared = daemon.diffie_hellman(&point);
    let mut bytes = Zeroizing::new([0; 32]);
    bytes.copy_from_slice(shared.raw_secret_bytes());
    Ok(bytes)
}

/// The 32 bytes of a fingerprint written as hex, with or without colons.
pub(crate) fn fingerprint_bytes(fingerprint: &str) -> Option<[u8; 32]> {
    let digits: Vec<u8> = fingerprint.bytes().filter(|byte| *byte != b':').collect();
    if digits.len() != 64 {
        return None;
    }
    let mut bytes = [0; 32];
    for (index, pair) in digits.chunks(2).enumerate() {
        let text = std::str::from_utf8(pair).ok()?;
        bytes[index] = u8::from_str_radix(text, 16).ok()?;
    }
    Some(bytes)
}

/// The fingerprint as 64 uppercase hex digits, no colons.
pub(crate) fn fingerprint_hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

/// What the daemon's second step produces, before encoding.
pub(crate) struct Revealed {
    pub(crate) code: String,
    pub(crate) sealed: Vec<u8>,
}

/// Both sides' values for one session: the daemon's key and the phone's
/// point, both nonces, and the CA fingerprint.
pub(crate) struct Exchange<'a> {
    pub(crate) daemon: &'a SecretKey,
    pub(crate) phone: &'a PublicKey,
    pub(crate) phone_nonce: &'a [u8; SECRET_LEN],
    pub(crate) daemon_nonce: &'a [u8; SECRET_LEN],
    pub(crate) fingerprint: &'a [u8; 32],
}

impl Exchange<'_> {
    /// The daemon's second step with every secret given: the code, and the
    /// pairing token sealed with the agreed key. The vectors call it
    /// directly.
    pub(crate) fn reveal(
        &self,
        token: &[u8; SECRET_LEN],
        origins: &[String],
        aes_nonce: &[u8; AES_NONCE_LEN],
    ) -> Result<Revealed> {
        let daemon_point = self.daemon.public_key().to_sec1_bytes();
        let transcript = transcript(
            self.phone.as_bytes(),
            &daemon_point,
            self.phone_nonce,
            self.daemon_nonce,
            self.fingerprint,
        );
        let shared = shared_secret(self.daemon, self.phone)?;
        let key = key(shared.as_slice(), &transcript)?;
        // A struct, not a map: a map's key order follows whether another
        // crate in the build turned on serde_json's `preserve_order`, and
        // these bytes are sealed and checked against shared vectors.
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&Offer {
                token: encode(token),
                origins,
            })
            .map_err(|_| Error::NearbyCrypto)?,
        );
        Ok(Revealed {
            code: code(&transcript),
            sealed: seal(&key, aes_nonce, &plaintext)?,
        })
    }
}

/// The daemon's half of a session between the phone's two steps.
#[derive(Debug)]
struct Session {
    id: String,
    secret: SecretKey,
    phone: PublicKey,
    nonce: [u8; SECRET_LEN],
    expires_ms: u64,
}

/// Sessions started and not yet revealed, at most [`MAX_SESSIONS`].
#[derive(Debug, Default)]
pub(crate) struct NearbySessions {
    sessions: Vec<Session>,
}

impl NearbySessions {
    /// The first step: a fresh key and nonce, and the commitment to them.
    pub(crate) fn start(
        &mut self,
        phone_key: &str,
        computer: &str,
        fingerprint: &str,
        now_ms: u64,
    ) -> Result<NearbyStarted> {
        let phone = PublicKey::parse(phone_key)?;
        let fingerprint = fingerprint_bytes(fingerprint).ok_or(Error::NearbyCrypto)?;
        self.sessions.retain(|session| session.expires_ms > now_ms);
        if self.sessions.len() >= MAX_SESSIONS {
            return Err(Error::NearbyBusy);
        }
        let secret = random_secret()?;
        let nonce = random_bytes::<SECRET_LEN>()?;
        let daemon_point = secret.public_key().to_sec1_bytes();
        let id = random_id()?;
        let started = NearbyStarted {
            session: id.clone(),
            computer: computer.to_owned(),
            fingerprint: fingerprint_hex(&fingerprint),
            public_key: encode(&daemon_point),
            commit: encode(&commit(&daemon_point, phone.as_bytes(), &nonce)),
        };
        self.sessions.push(Session {
            id,
            secret,
            phone,
            nonce,
            expires_ms: now_ms.saturating_add(SESSION_TTL_MS),
        });
        Ok(started)
    }

    /// Takes the session `id` for its second step; it is forgotten either
    /// way, so a nonce is revealed once.
    pub(crate) fn take(&mut self, id: &str, now_ms: u64) -> Result<StartedSession> {
        self.sessions.retain(|session| session.expires_ms > now_ms);
        let index = self
            .sessions
            .iter()
            .position(|session| constant_time_eq(session.id.as_bytes(), id.as_bytes()))
            .ok_or(Error::UnknownNearby)?;
        let session = self.sessions.swap_remove(index);
        Ok(StartedSession {
            secret: session.secret,
            phone: session.phone,
            nonce: session.nonce,
        })
    }
}

/// A session's secrets, taken for its second step.
pub(crate) struct StartedSession {
    pub(crate) secret: SecretKey,
    pub(crate) phone: PublicKey,
    pub(crate) nonce: [u8; SECRET_LEN],
}

impl StartedSession {
    /// The second step for the phone's nonce: seals `token` to it and
    /// answers what the phone needs, with the code the claim will carry.
    pub(crate) fn reveal(
        self,
        phone_nonce: &str,
        fingerprint: &str,
        token: &[u8; SECRET_LEN],
        origins: &[String],
    ) -> Result<(NearbyRevealed, String)> {
        let phone_nonce: [u8; SECRET_LEN] =
            decode_fixed(phone_nonce).map_err(|_| Error::NearbyCrypto)?;
        let fingerprint = fingerprint_bytes(fingerprint).ok_or(Error::NearbyCrypto)?;
        let aes_nonce = random_bytes::<AES_NONCE_LEN>()?;
        let revealed = Exchange {
            daemon: &self.secret,
            phone: &self.phone,
            phone_nonce: &phone_nonce,
            daemon_nonce: &self.nonce,
            fingerprint: &fingerprint,
        }
        .reveal(token, origins, &aes_nonce)?;
        Ok((
            NearbyRevealed {
                nonce: encode(&self.nonce),
                sealed: encode(&revealed.sealed),
            },
            revealed.code,
        ))
    }
}

/// A random P-256 secret key from the system's randomness.
fn random_secret() -> Result<SecretKey> {
    // A random 32-byte value is a valid scalar but for odds of about 2^-32.
    loop {
        if let Ok(secret) = SecretKey::from_slice(&random_bytes::<32>()?) {
            return Ok(secret);
        }
    }
}

/// What the sealed offer holds: a claim token and where to claim it.
#[derive(serde::Serialize)]
pub(crate) struct Offer<'a> {
    pub(crate) token: String,
    pub(crate) origins: &'a [String],
}
